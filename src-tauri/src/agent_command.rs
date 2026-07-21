//! Agent commands exposed to the Tauri frontend.
//!
//! Provides `start_agent_turn` (multi-turn AI agent with streaming events)
//! and `approve_tool_call` (human-in-the-loop for dangerous tools).
//!
//! Architecture:
//! - Sessions are stored in a global Arc<Mutex<HashMap>> shared between
//!   commands and background tasks.
//! - Each `start_agent_turn` spawns a tokio task that runs Agent::run().
//! - The callback emits events via AppHandle::emit() using session-scoped
//!   event names: `agent://{session_id}/token`, etc.
//! - Dangerous tools (write, edit, shell) wait for user approval via oneshot
//!   channels stored in the session's approval map.
//! - Write/edit operations targeting the user's currently-open file skip the
//!   confirmation dialog (smart auto-approve). The session's `current_file`
//!   and `cwd` are updated each turn from the frontend.
//! - Read-only tools (read, grep, find, ls) execute immediately.

use crate::agent::{Agent, AgentConfig, AgentEvent};
use crate::error::Result as AgentResult;
use crate::model::{
    AssistantMessage, ContentBlock, Message, StopReason, TextContent, Usage, UserContent,
    UserMessage,
};
use crate::provider::StreamOptions;
use crate::runtime_env::{self, RuntimeState};
use crate::settings;
use crate::skills;
use crate::tools::{self, Tool, ToolEffects, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

// ============================================================================
// Frontend-facing event types (lightweight, serializable)
// ============================================================================

/// Events emitted from the agent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentFrontendEvent {
    Token {
        delta: String,
    },
    Thinking {
        delta: String,
    },
    ToolCall {
        #[serde(rename = "callId")]
        call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        arguments: serde_json::Value,
        /// Set when this is a read of a SKILL.md file (skill invocation).
        #[serde(rename = "skillName", skip_serializing_if = "Option::is_none")]
        skill_name: Option<String>,
    },
    ToolResult {
        #[serde(rename = "callId")]
        call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "isError")]
        is_error: bool,
        summary: String,
        /// Set when this result is from a skill invocation.
        #[serde(rename = "skillName", skip_serializing_if = "Option::is_none")]
        skill_name: Option<String>,
    },
    /// A dangerous tool needs user confirmation before execution.
    /// The agent pauses until approve_tool_call is invoked.
    ToolConfirmation {
        #[serde(rename = "callId")]
        call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        /// Human-readable summary of what will happen
        summary: String,
        /// Full details (diff, command, file path, etc.)
        details: serde_json::Value,
    },
    TurnEnd {
        #[serde(rename = "stopReason")]
        stop_reason: String,
        #[serde(rename = "inputTokens")]
        input_tokens: u64,
        #[serde(rename = "outputTokens")]
        output_tokens: u64,
    },
    Error {
        message: String,
    },
    /// Context compaction started (inline, blocks the current turn).
    CompactionStarted {
        reason: String,
        #[serde(rename = "tokensBefore")]
        tokens_before: u64,
    },
    /// Context compaction finished.
    CompactionFinished {
        #[serde(rename = "tokensAfter")]
        tokens_after: u64,
        #[serde(rename = "summaryLen")]
        summary_len: usize,
    },
}

// ============================================================================
// Session state
// ============================================================================

/// A pending approval request for a dangerous tool call.
pub(crate) struct PendingApproval {
    pub(crate) response_tx: oneshot::Sender<bool>,
}

/// Per-session data.
pub(crate) struct SessionData {
    pub(crate) agent: Option<Agent>,
    /// Map of call_id → oneshot sender for pending dangerous tool approvals.
    pub(crate) pending_approvals: Arc<Mutex<HashMap<String, PendingApproval>>>,
    pub(crate) auto_approve: Arc<AtomicBool>,
    /// Current file open in the editor (updated each turn for smart auto-approve).
    pub(crate) current_file: Arc<std::sync::Mutex<Option<String>>>,
    /// Working directory (updated each turn from the frontend).
    pub(crate) cwd: Arc<std::sync::Mutex<PathBuf>>,
    /// Cancellation token — fired when the session should stop.
    pub(crate) cancellation_token: CancellationToken,
}

// ============================================================================
// Session persistence: JSONL storage under ~/.config/zcode/sessions/
//
// Assumptions: single-window, single-instance. No concurrent write protection.
// ============================================================================

/// Simplified message for disk storage. Only user/assistant messages are persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    id: String,
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens: Option<u64>,
    timestamp: i64,
}

/// Convert a persisted ChatMessage back into a model Message for seeding agent history.
fn chat_message_to_message(msg: &ChatMessage) -> Option<Message> {
    match msg.role.as_str() {
        "user" => Some(Message::User(UserMessage {
            content: UserContent::Text(msg.content.clone()),
            timestamp: msg.timestamp,
        })),
        "assistant" => Some(Message::assistant(AssistantMessage {
            content: vec![ContentBlock::Text(TextContent::new(msg.content.clone()))],
            api: String::new(),
            provider: String::new(),
            model: String::new(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: msg.timestamp,
        })),
        _ => None,
    }
}

// ============================================================================
// Session listing: enumerate all saved sessions with metadata
// ============================================================================

/// Lightweight session metadata for history list display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub session_key: String,
    /// Title derived from the first user message (truncated to ~60 chars).
    pub title: String,
    /// Timestamp of the most recent message in this session.
    pub timestamp: i64,
    /// Number of user + assistant messages.
    pub message_count: usize,
}

/// List saved sessions with metadata, optionally filtered to a workspace (cwd).
/// When `cwd` is Some(dir), only sessions under that directory are returned.
/// Sessions are sorted by most recent first.
#[tauri::command]
pub fn list_sessions(cwd: Option<String>) -> Result<Vec<SessionMeta>, String> {
    let folder_prefix = cwd.as_deref().map(compute_folder_prefix);
    let sessions_dir = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("zcode")
        .join("sessions");

    if !sessions_dir.exists() {
        return Ok(vec![]);
    }

    let mut metas = Vec::new();
    let entries = fs::read_dir(&sessions_dir).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }
        let session_key = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if session_key.is_empty() {
            continue;
        }
        // Filter by folder prefix when requested
        if let Some(ref prefix) = folder_prefix {
            if !session_key.starts_with(prefix) {
                continue;
            }
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let messages: Vec<ChatMessage> = content
            .lines()
            .filter_map(|l| serde_json::from_str::<ChatMessage>(l).ok())
            .collect();

        if messages.is_empty() {
            continue;
        }

        // Title = first user message, truncated
        let title = messages
            .iter()
            .find(|m| m.role == "user")
            .map(|m| {
                let t = m.content.trim();
                if t.len() > 60 {
                    format!("{}…", t.chars().take(57).collect::<String>())
                } else {
                    t.to_string()
                }
            })
            .unwrap_or_else(|| "New conversation".to_string());

        let timestamp = messages.last().map(|m| m.timestamp).unwrap_or(0);
        let message_count = messages
            .iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .count();

        metas.push(SessionMeta {
            session_key,
            title,
            timestamp,
            message_count,
        });
    }

    // Sort by most recent first
    metas.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(metas)
}

/// Compute a folder-based prefix from a workspace directory path.
/// Uses dunce::canonicalize + sha256(first 16 chars).
fn compute_folder_prefix(dir: &str) -> String {
    let path = Path::new(dir);
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..16].to_string()
}

/// Find the latest existing session key for a folder prefix.
/// Returns None if no session exists for this folder yet.
fn find_latest_session_for_prefix(prefix: &str) -> Option<String> {
    let sessions_dir = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("zcode")
        .join("sessions");
    if !sessions_dir.exists() {
        return None;
    }
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem.starts_with(prefix) {
                    candidates.push(stem.to_string());
                }
            }
        }
    }
    // Sort by embedded timestamp suffix (descending) = newest first
    candidates.sort_by(|a, b| b.cmp(a));
    candidates.into_iter().next()
}

/// Create a brand-new session key for a folder (prefix + timestamp).
fn new_session_key_for_prefix(prefix: &str) -> String {
    format!("{prefix}-{}", chrono::Utc::now().timestamp_millis())
}

/// Filesystem path for a session's JSONL file.
fn session_file_path(session_key: &str) -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("zcode")
        .join("sessions")
        .join(format!("{session_key}.jsonl"))
}

static SESSIONS_DIR_CREATED: AtomicBool = AtomicBool::new(false);

/// Generate a unique message ID using microsecond timestamp.
/// This avoids duplicate keys across process restarts (unlike a global counter).
fn next_msg_id(role: &str) -> String {
    let ts = chrono::Utc::now().timestamp_micros();
    format!("{role}-{ts}")
}

/// Append a single message to the session JSONL file (append-only).
/// Only user/assistant messages are persisted; tool/error messages are silently skipped.
fn append_session_message(session_key: &str, msg: &ChatMessage) -> std::io::Result<()> {
    if !matches!(msg.role.as_str(), "user" | "assistant") {
        return Ok(());
    }
    let path = session_file_path(session_key);
    if !SESSIONS_DIR_CREATED.load(Ordering::Relaxed) {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        SESSIONS_DIR_CREATED.store(true, Ordering::Relaxed);
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let line = serde_json::to_string(msg).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")?;
    Ok(())
}

#[tauri::command]
pub fn load_session_messages(session_key: String) -> Result<Vec<ChatMessage>, String> {
    let path = session_file_path(&session_key);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let base_ts = chrono::Utc::now().timestamp_millis();
    Ok(content
        .lines()
        .filter_map(|l| serde_json::from_str::<ChatMessage>(l).ok())
        .enumerate()
        .map(|(i, mut msg)| {
            // Regenerate ID to guarantee uniqueness across restarts.
            msg.id = format!("{}-{}-{}", msg.role, base_ts, i);
            msg
        })
        .collect())
}

#[tauri::command]
pub fn resolve_session_key(cwd: String) -> Result<String, String> {
    let prefix = compute_folder_prefix(&cwd);
    // Reuse the latest session for this workspace, or create a new one
    Ok(find_latest_session_for_prefix(&prefix)
        .unwrap_or_else(|| new_session_key_for_prefix(&prefix)))
}

/// Create a brand-new session key for the workspace `cwd`.
/// This does NOT touch any existing sessions — it always creates a new key.
/// Used by the frontend "New Agent" button.
#[tauri::command]
pub fn new_session_key(cwd: String) -> Result<String, String> {
    let prefix = compute_folder_prefix(&cwd);
    Ok(new_session_key_for_prefix(&prefix))
}

#[tauri::command]
pub async fn clear_session(
    session_key: String,
    state: tauri::State<'_, SessionManager>,
) -> Result<(), String> {
    let path = session_file_path(&session_key);
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    let mut map = state.sessions.lock().await;
    if let Some(sd) = map.get_mut(&session_key) {
        if let Some(ref mut agent) = sd.agent {
            agent.clear_messages();
        }
    }
    Ok(())
}

// ============================================================================
// Session Manager
// ============================================================================

/// Global session map, shared between commands and background tasks.
type SessionMap = Arc<Mutex<HashMap<String, SessionData>>>;

/// Tauri managed state wrapper.
pub struct SessionManager {
    pub(crate) sessions: SessionMap,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// ============================================================================
// Guarded Tool — intercepts dangerous tools for approval
// ============================================================================

/// A tool wrapper that requires user approval for dangerous operations.
struct GuardedTool {
    inner: Box<dyn Tool>,
    /// Shared approval map for this session.
    pending_approvals: Arc<Mutex<HashMap<String, PendingApproval>>>,
    /// Auto-approve flag (if true, skip confirmation for all tools).
    auto_approve: Arc<AtomicBool>,
    /// AppHandle for emitting frontend events.
    app: AppHandle,
    /// Session ID for scoped events.
    session_id: String,
    /// The file currently open in the editor (shared, updated each turn).
    /// Write/edit targeting this file skip the confirmation dialog.
    current_file: Arc<std::sync::Mutex<Option<String>>>,
    /// Working directory (shared, updated each turn).
    cwd: Arc<std::sync::Mutex<PathBuf>>,
}

impl GuardedTool {
    fn is_dangerous(&self) -> bool {
        let name = self.inner.name();
        name == "write" || name == "edit" || name == "shell"
    }

    /// Returns true when the tool's target path matches the currently-open file.
    /// Only applies to write/edit (not shell). Relative paths are resolved against `cwd`.
    fn targets_current_file(&self, input: &serde_json::Value) -> bool {
        let name = self.inner.name();
        if name != "write" && name != "edit" {
            return false;
        }
        let current = self.current_file.lock().unwrap();
        let Some(ref current) = *current else {
            return false;
        };
        let Some(target) = input.get("path").and_then(|v| v.as_str()) else {
            return false;
        };

        let cwd = self.cwd.lock().unwrap();
        let absolute = if std::path::Path::new(target).is_absolute() {
            std::path::PathBuf::from(target)
        } else {
            cwd.join(target)
        };
        drop(cwd);

        let cur = std::path::Path::new(current);
        match (dunce::canonicalize(cur), dunce::canonicalize(&absolute)) {
            (Ok(c), Ok(t)) => c == t,
            _ => cur == absolute.as_path(),
        }
    }
}

#[async_trait]
impl Tool for GuardedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> serde_json::Value {
        self.inner.parameters()
    }

    fn effects(&self) -> ToolEffects {
        self.inner.effects()
    }

    async fn execute(
        &self,
        tool_call_id: &str,
        input: serde_json::Value,
        on_update: Option<Box<dyn Fn(tools::ToolUpdate) + Send + Sync>>,
    ) -> AgentResult<ToolOutput> {
        // Auto-approve if: global flag is on, tool is read-only,
        // OR the target file is the currently-open file.
        if self.auto_approve.load(Ordering::Relaxed)
            || !self.is_dangerous()
            || self.targets_current_file(&input)
        {
            return self.inner.execute(tool_call_id, input, on_update).await;
        }

        // Build confirmation details
        let summary = build_confirmation_summary(self.name(), &input);
        let details = build_confirmation_details(self.name(), &input);

        // Emit confirmation event to frontend
        let call_id = tool_call_id.to_string();
        let tool_name = self.name().to_string();
        let _ = self.app.emit(
            &format!("agent://{}/tool-confirmation", self.session_id),
            AgentFrontendEvent::ToolConfirmation {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                summary,
                details,
            },
        );

        // Create oneshot channel and register in approval map
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending_approvals.lock().await;
            map.insert(call_id.clone(), PendingApproval { response_tx: tx });
        }

        // Wait for user decision (with 5-minute timeout as safety)
        let approved = matches!(
            tokio::time::timeout(std::time::Duration::from_secs(300), rx).await,
            Ok(Ok(true))
        );

        // Clean up the pending entry
        {
            let mut map = self.pending_approvals.lock().await;
            map.remove(&call_id);
        }

        if approved {
            self.inner.execute(tool_call_id, input, on_update).await
        } else {
            Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new(
                    "Tool execution was rejected by the user.",
                ))],
                details: None,
                is_error: true,
            })
        }
    }
}

/// Build a human-readable summary for confirmation.
fn build_confirmation_summary(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "write" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown file");
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let preview: String = content
                .lines()
                .take(3)
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");
            format!("Write to `{}`:\n{}", path, preview)
        }
        "edit" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown file");
            let old_text = input.get("oldText").and_then(|v| v.as_str()).unwrap_or("");
            let new_text = input.get("newText").and_then(|v| v.as_str()).unwrap_or("");
            let old_preview = safe_truncate(old_text, 80);
            let new_preview = safe_truncate(new_text, 80);
            format!(
                "Edit `{}`:\n- {} \n+ {}",
                path,
                old_preview.replace('\n', "\n- "),
                new_preview.replace('\n', "\n+ ")
            )
        }
        "shell" => {
            let cmd = input
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown command");
            format!("Run command: `{}`", cmd)
        }
        _ => format!("Execute tool `{tool_name}`"),
    }
}

/// Build detailed info for the confirmation dialog.
fn build_confirmation_details(tool_name: &str, input: &serde_json::Value) -> serde_json::Value {
    match tool_name {
        "write" => serde_json::json!({
            "path": input.get("path"),
            "content": input.get("content"),
        }),
        "edit" => serde_json::json!({
            "path": input.get("path"),
            "oldText": input.get("oldText"),
            "newText": input.get("newText"),
        }),
        "shell" => serde_json::json!({
            "command": input.get("command"),
            "timeout": input.get("timeout"),
        }),
        _ => serde_json::json!({}),
    }
}

// ============================================================================
// Helper: build guarded tool registry
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn build_guarded_registry(
    tool_names: &[&str],
    cwd: &std::path::Path,
    pending_approvals: Arc<Mutex<HashMap<String, PendingApproval>>>,
    auto_approve: Arc<AtomicBool>,
    app: AppHandle,
    session_id: &str,
    shared_current_file: Arc<std::sync::Mutex<Option<String>>>,
    shared_cwd: Arc<std::sync::Mutex<PathBuf>>,
    augmented_path: Option<&str>,
    venv_dir: Option<&Path>,
) -> ToolRegistry {
    use crate::tools::{
        bash::BashTool, edit::EditTool, find::FindTool, grep::GrepTool, ls::LsTool, read::ReadTool,
        write::WriteTool,
    };

    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    for name in tool_names {
        let tool: Box<dyn Tool> = match *name {
            "read" => Box::new(ReadTool::new(cwd)),
            "shell" => {
                if let (Some(path), Some(venv)) = (augmented_path, venv_dir) {
                    Box::new(BashTool::with_runtime(
                        cwd,
                        path.to_string(),
                        venv.to_path_buf(),
                    ))
                } else {
                    Box::new(BashTool::new(cwd))
                }
            }
            "edit" => Box::new(EditTool::new(cwd)),
            "write" => Box::new(WriteTool::new(cwd)),
            "grep" => Box::new(GrepTool::new(cwd)),
            "find" => Box::new(FindTool::new(cwd)),
            "ls" => Box::new(LsTool::new(cwd)),
            _ => continue,
        };
        let guarded: Box<dyn Tool> = Box::new(GuardedTool {
            inner: tool,
            pending_approvals: Arc::clone(&pending_approvals),
            auto_approve: Arc::clone(&auto_approve),
            app: app.clone(),
            session_id: session_id.to_string(),
            current_file: Arc::clone(&shared_current_file),
            cwd: Arc::clone(&shared_cwd),
        });
        tools.push(guarded);
    }
    ToolRegistry::from_tools(tools)
}

// ============================================================================
// Helper: build system prompt with skills
// ============================================================================
// Skill state persistence (~/.config/zcode/skill-state.json)
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SkillState {
    disabled: Vec<String>,
}

fn skill_state_path(user_config_dir: &Path) -> PathBuf {
    user_config_dir.join("skill-state.json")
}

fn load_skill_state(user_config_dir: &Path) -> SkillState {
    std::fs::read_to_string(skill_state_path(user_config_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_skill_state(user_config_dir: &Path, state: &SkillState) -> std::io::Result<()> {
    std::fs::create_dir_all(user_config_dir)?;
    let json =
        serde_json::to_string_pretty(state).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(skill_state_path(user_config_dir), json)
}

/// A skill combined with its enabled/disabled state.
#[derive(Debug, Clone, Serialize)]
pub struct SkillWithState {
    #[serde(flatten)]
    pub skill: skills::Skill,
    pub active: bool,
}

/// List all discovered skills with their enabled/disabled state.
#[tauri::command]
pub async fn list_skills(cwd: String) -> Result<Vec<SkillWithState>, String> {
    let cwd = PathBuf::from(cwd);
    let user_dir = dirs::config_dir()
        .map(|d| d.join("zcode"))
        .ok_or("No config directory found")?;
    let (found, diags) = skills::load_skills(&cwd, Some(&user_dir), &[]);
    for d in &diags {
        eprintln!("[zcode] skill diag: {d}");
    }
    let state = load_skill_state(&user_dir);
    Ok(found
        .into_iter()
        .map(|s| {
            let active = !state.disabled.contains(&s.name);
            SkillWithState { skill: s, active }
        })
        .collect())
}

/// Enable or disable a skill by name (persists to skill-state.json).
static SKILL_STATE_LOCK: LazyLock<StdMutex<()>> = LazyLock::new(|| StdMutex::new(()));

#[tauri::command]
pub async fn set_skill_active(name: String, active: bool) -> Result<(), String> {
    let _guard = SKILL_STATE_LOCK.lock().map_err(|e| e.to_string())?;
    let user_dir = dirs::config_dir()
        .map(|d| d.join("zcode"))
        .ok_or("No config directory found")?;
    let mut state = load_skill_state(&user_dir);
    if active {
        state.disabled.retain(|n| n != &name);
    } else if !state.disabled.contains(&name) {
        state.disabled.push(name);
    }
    save_skill_state(&user_dir, &state).map_err(|e| e.to_string())
}

// ============================================================================

fn build_system_prompt(
    cwd: &Path,
    current_file: Option<&str>,
    user_config_dir: Option<&Path>,
    pin_folder: Option<&str>,
    scripts_folder: Option<&str>,
    sources_folder: Option<&str>,
    output_folder: Option<&str>,
) -> String {
    // Static prompt from a standalone file — edit src/prompts/system.md to tweak.
    let base = include_str!("prompts/system.md");
    let mut prompt = String::with_capacity(base.len() + 2048);
    prompt.push_str(base);

    // --- Dynamic: session context ---
    prompt.push_str("\n**Important:** The Current Session section below reflects live editor\n");
    prompt.push_str("state that may change between turns when the user switches files. It is\n");
    prompt.push_str("NOT a correction of earlier messages — the user simply opened a different\n");
    prompt.push_str("file since your last turn. Do not apologize for previous answers; just\n");
    prompt.push_str("use the current value.\n");
    prompt.push_str("\n## Current Session\n\n");
    prompt.push_str(&format!("Working directory: {}\n", cwd.display()));

    if let Some(path) = current_file {
        prompt.push_str(&format!(
            "The user has this file open: `{path}`\n\
             You may freely read and edit this file. Editing other files \
             requires the user's confirmation.\n"
        ));
    } else {
        prompt.push_str("No file is open. All edits will require user confirmation.\n");
    }

    prompt.push('\n');

    // --- Dynamic: shell environment ---
    prompt.push_str("## Shell Environment\n\n");
    if cfg!(windows) {
        prompt.push_str(include_str!("prompts/windows_shell.md"));
    } else {
        prompt.push_str(
            "The `shell` tool runs commands through a POSIX shell (bash/sh) on \
             this Unix-like system.\n",
        );
    }
    prompt.push('\n');

    // --- Dynamic: workspace folders ---
    if pin_folder.is_some()
        || scripts_folder.is_some()
        || sources_folder.is_some()
        || output_folder.is_some()
    {
        prompt.push_str("## Workspace Folders\n\n");
        prompt.push_str("This project uses a fixed four-folder convention:\n");
        if let Some(p) = pin_folder {
            prompt.push_str(&format!("- Markdown notes folder: `{p}`\n"));
        }
        if let Some(p) = scripts_folder {
            prompt.push_str(&format!(
                "- Scripts folder (for scripts you write to complete tasks): `{p}`\n"
            ));
        }
        if let Some(p) = sources_folder {
            prompt.push_str(&format!("- Sources folder (staging area for existing non-md files the user wants you to edit): `{p}`\n"));
        }
        if let Some(p) = output_folder {
            prompt.push_str(&format!("- Output folder (script-generated non-md artifacts only, e.g. images, generated docs): `{p}`\n"));
        }
        prompt.push_str("\nRules:\n");
        prompt.push_str(
            "- Save any script you write into the scripts folder, not next to the user's notes.\n",
        );
        prompt.push_str("- Any non-md file produced by a script (image, generated document, etc.) belongs in the output folder. Never save script output next to markdown notes.\n");
        prompt.push_str("- If the user asks you to modify an existing non-markdown file (Excel, Word, PDF, etc.) that lives outside the sources folder, first copy it into the sources folder, then read/edit the copy there. Do not edit files outside the sources folder directly.\n");
        prompt.push_str("- The markdown notes folder is for the user's own documents — don't create scripts or dump generated artifacts there.\n");
        prompt.push('\n');
    }

    // --- Dynamic: skills ---
    let (all_skills, _diags) = skills::load_skills(cwd, user_config_dir, &[]);
    let enabled: Vec<_> = match user_config_dir {
        Some(dir) => {
            let state = load_skill_state(dir);
            all_skills
                .into_iter()
                .filter(|s| !state.disabled.contains(&s.name))
                .collect()
        }
        None => all_skills,
    };

    if !enabled.is_empty() {
        eprintln!(
            "[zcode] build_system_prompt: injecting {} enabled skill(s): {:?}",
            enabled.len(),
            enabled.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        prompt.push_str(&skills::format_skills_for_prompt(&enabled));
    }

    prompt
}

// ============================================================================
// Helper: detect skill invocation from read tool calls
// ============================================================================

/// Check if a read tool call is targeting a SKILL.md file and extract the skill name.
fn detect_skill_invocation(tool_name: &str, arguments: &serde_json::Value) -> Option<String> {
    if tool_name != "read" {
        return None;
    }
    let path_str = arguments.get("path")?.as_str()?;
    let path = std::path::Path::new(path_str);
    if path.file_name()?.to_str()? != "SKILL.md" {
        return None;
    }
    path.parent()?.file_name()?.to_str().map(|s| s.to_string())
}

// ============================================================================
// Helper: extract text summary from tool result
// ============================================================================

fn safe_truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        format!("{}...", tools::truncate_at_char_boundary(s, max_bytes))
    }
}

fn tool_result_summary(content: &[ContentBlock]) -> String {
    for block in content {
        if let ContentBlock::Text(tc) = block {
            let text = tc.text.trim();
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("...") {
                    continue;
                }
                if trimmed.len() > 120 {
                    return safe_truncate(trimmed, 120);
                }
                return trimmed.to_string();
            }
            if text.len() > 120 {
                return safe_truncate(text, 120);
            }
            return text.to_string();
        }
    }
    "(non-text result)".to_string()
}

/// Remove a session's in-memory state from the SessionManager.
/// Signals cancellation to any running tokio task, then removes the entry.
#[tauri::command]
pub async fn close_session(
    session_key: String,
    state: tauri::State<'_, SessionManager>,
) -> Result<(), String> {
    let mut map = state.sessions.lock().await;
    if let Some(sd) = map.get(&session_key) {
        sd.cancellation_token.cancel();
    }
    map.remove(&session_key);
    Ok(())
}

/// Cancel ALL active sessions and clear the session map.
/// Called when the Agent panel is closed or the app exits.
#[tauri::command]
pub async fn close_all_sessions(state: tauri::State<'_, SessionManager>) -> Result<(), String> {
    let mut map = state.sessions.lock().await;
    for (key, sd) in map.iter() {
        eprintln!("[zcode] close_all_sessions: cancelling {key}");
        sd.cancellation_token.cancel();
    }
    map.clear();
    Ok(())
}

// ============================================================================
// Command: start_agent_turn
// ============================================================================

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_agent_turn(
    app: AppHandle,
    state: tauri::State<'_, SessionManager>,
    runtime_state: tauri::State<'_, RuntimeState>,
    session_id: String,
    user_message: String,
    allowed_tools: Vec<String>,
    base_url: String,
    model: String,
    provider_name: Option<String>,
    current_file: Option<String>,
    cwd: Option<String>,
    auto_approve_writes: Option<bool>,
    context_window_tokens: Option<u32>,
    pin_folder: Option<String>,
    scripts_folder: Option<String>,
    sources_folder: Option<String>,
    output_folder: Option<String>,
) -> Result<(), String> {
    eprintln!("[zcode] start_agent_turn: session={session_id}, base_url={base_url}, model={model}, msg_len={}", user_message.len());
    eprintln!(
        "[zcode] start_agent_turn: tools={allowed_tools:?}, auto_approve={auto_approve_writes:?}"
    );

    if user_message.trim().is_empty() {
        eprintln!("[zcode] start_agent_turn: ERROR empty user message");
        return Err("User message cannot be empty".to_string());
    }
    if base_url.is_empty() {
        eprintln!("[zcode] start_agent_turn: ERROR empty base_url");
        return Err("No Base URL configured".to_string());
    }
    if model.is_empty() {
        eprintln!("[zcode] start_agent_turn: ERROR empty model");
        return Err("No model configured".to_string());
    }

    // Save the user message to disk (best-effort, don't block the turn on I/O error)
    let _ = append_session_message(
        &session_id,
        &ChatMessage {
            id: next_msg_id("user"),
            role: "user".to_string(),
            content: user_message.clone(),
            input_tokens: None,
            output_tokens: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        },
    );

    let name = provider_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "openai".to_string());

    eprintln!("[zcode] start_agent_turn: reading API key from keychain...");
    let api_key = match settings::get_api_key() {
        Ok(Some(key)) => {
            eprintln!(
                "[zcode] start_agent_turn: API key found (len={})",
                key.len()
            );
            key
        }
        Ok(None) => {
            eprintln!("[zcode] start_agent_turn: ERROR no API key in keychain");
            return Err("No API key configured".to_string());
        }
        Err(e) => {
            eprintln!("[zcode] start_agent_turn: ERROR reading keychain: {e}");
            return Err(e);
        }
    };

    let work_dir = if let Some(ref d) = cwd {
        PathBuf::from(d)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    // --- Ensure bundled runtime (Python + uv + Bun) is initialized ---
    let runtime_opt: runtime_env::AgentRuntime = {
        let cached = runtime_state.runtime.lock().unwrap().clone();
        if let Some(rt) = cached {
            rt
        } else {
            match runtime_env::ensure_agent_venv(&app).await {
                Ok(rt) => {
                    eprintln!(
                        "[zcode] Bundled runtime initialized: venv={}",
                        rt.venv_dir.display()
                    );
                    let mut guard = runtime_state.runtime.lock().unwrap();
                    if guard.is_none() {
                        *guard = Some(rt.clone());
                    }
                    rt
                }
                Err(e) => {
                    eprintln!("[zcode] ERROR: Failed to init bundled runtime: {e}");
                    return Err(format!(
                        "内置运行时初始化失败：{}\n\n请检查网络连接后重试，或手动运行 scripts/fetch-runtime/ 脚本下载运行时。",
                        e
                    ));
                }
            }
        }
    };
    let augmented_path: String = runtime_env::augmented_path(&runtime_opt);
    let runtime_venv_dir: PathBuf = runtime_opt.venv_dir.clone();

    // Build system prompt
    let user_config_dir = dirs::config_dir().map(|d| d.join("zcode"));
    let system_prompt = build_system_prompt(
        &work_dir,
        current_file.as_deref(),
        user_config_dir.as_deref(),
        pin_folder.as_deref(),
        scripts_folder.as_deref(),
        sources_folder.as_deref(),
        output_folder.as_deref(),
    );
    eprintln!(
        "[zcode] start_agent_turn: system_prompt len={}, cwd={}",
        system_prompt.len(),
        work_dir.display()
    );

    // Register workspace roots so file tools can access sources/scripts/output
    // which live alongside (not inside) the pin folder.
    let mut workspace_roots: Vec<std::path::PathBuf> = Vec::new();
    if let Some(p) = &sources_folder {
        workspace_roots.push(std::path::PathBuf::from(p));
    }
    if let Some(p) = &scripts_folder {
        workspace_roots.push(std::path::PathBuf::from(p));
    }
    if let Some(p) = &output_folder {
        workspace_roots.push(std::path::PathBuf::from(p));
    }
    crate::tools::set_workspace_roots(workspace_roots);

    // Build provider
    eprintln!("[zcode] start_agent_turn: building provider (name={name})...");
    let provider =
        crate::providers::build_provider(&name, &model, &api_key, &base_url).map_err(|e| {
            eprintln!("[zcode] start_agent_turn: ERROR building provider: {e}");
            e.to_string()
        })?;
    eprintln!(
        "[zcode] start_agent_turn: provider built: name={}, api={}, model_id={}",
        provider.name(),
        provider.api(),
        provider.model_id()
    );

    // Agent config (clone system_prompt so we can also use it when reusing an agent)
    let config = AgentConfig {
        system_prompt: Some(system_prompt.clone()),
        max_tool_iterations: 50,
        stream_options: StreamOptions {
            session_id: Some(session_id.clone()),
            ..Default::default()
        },
    };

    // Get or create session
    let sessions = state.sessions.clone();
    let mut map = sessions.lock().await;

    let allowed_tools_for_rebuild: Vec<String> = if allowed_tools.is_empty() {
        vec!["read", "write", "edit", "shell", "grep", "find", "ls"]
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        allowed_tools.clone()
    };

    let auto_approve_arc: Arc<AtomicBool>;
    let pending_approvals_arc: Arc<Mutex<HashMap<String, PendingApproval>>>;
    let current_file_arc: Arc<std::sync::Mutex<Option<String>>>;
    let cwd_arc: Arc<std::sync::Mutex<PathBuf>>;

    let cancel_token: CancellationToken;
    let mut agent = if let Some(sd) = map.get_mut(&session_id) {
        cancel_token = sd.cancellation_token.clone();
        sd.auto_approve
            .store(auto_approve_writes.unwrap_or(false), Ordering::Relaxed);
        auto_approve_arc = Arc::clone(&sd.auto_approve);
        pending_approvals_arc = Arc::clone(&sd.pending_approvals);
        current_file_arc = Arc::clone(&sd.current_file);
        cwd_arc = Arc::clone(&sd.cwd);

        // Update shared state for the new turn
        *current_file_arc.lock().unwrap() = current_file.clone();
        *cwd_arc.lock().unwrap() = work_dir.clone();

        // Update system prompt so the LLM sees the latest current_file
        let mut reused = sd
            .agent
            .take()
            .ok_or_else(|| "Agent is already running for this session".to_string())?;
        reused.set_system_prompt(Some(system_prompt));

        // Update compaction window if caller provided one
        if let Some(window) = context_window_tokens {
            reused.set_compaction_settings(Some(crate::compaction::CompactionSettings {
                context_window_tokens: window,
                ..Default::default()
            }));
        }

        reused
    } else {
        auto_approve_arc = Arc::new(AtomicBool::new(auto_approve_writes.unwrap_or(false)));
        pending_approvals_arc = Arc::new(Mutex::new(HashMap::new()));
        current_file_arc = Arc::new(std::sync::Mutex::new(current_file.clone()));
        cwd_arc = Arc::new(std::sync::Mutex::new(work_dir.clone()));

        cancel_token = CancellationToken::new();

        // Build guarded tool registry
        let tool_names: Vec<&str> = allowed_tools_for_rebuild
            .iter()
            .map(|s| s.as_str())
            .collect();

        let tool_registry = build_guarded_registry(
            &tool_names,
            &work_dir,
            Arc::clone(&pending_approvals_arc),
            Arc::clone(&auto_approve_arc),
            app.clone(),
            &session_id,
            Arc::clone(&current_file_arc),
            Arc::clone(&cwd_arc),
            Some(&augmented_path),
            Some(&runtime_venv_dir),
        );

        let mut agent = Agent::new(Arc::clone(&provider), tool_registry, config.clone());

        // Seed agent with existing conversation history from disk (first turn only).
        // The current user message was just appended to the JSONL above;
        // agent.run() will push it into history itself, so skip it here.
        if let Ok(history) = crate::agent_command::load_session_messages(session_id.clone()) {
            let mut history_messages: Vec<Message> =
                history.iter().filter_map(chat_message_to_message).collect();
            if matches!(history_messages.last(), Some(Message::User(_))) {
                history_messages.pop();
            }
            if !history_messages.is_empty() {
                eprintln!(
                    "[zcode] start_agent_turn: seeding {} history messages for session={session_id}",
                    history_messages.len()
                );
                agent.seed_history(history_messages);
            }
        }

        // Configure compaction: use explicit window if provided, else guess from model name
        let window = context_window_tokens
            .unwrap_or_else(|| crate::compaction::CompactionSettings::guess_from_model(&model));
        let cs = crate::compaction::CompactionSettings {
            context_window_tokens: window,
            ..Default::default()
        };
        agent.set_compaction_settings(Some(cs));

        map.insert(
            session_id.clone(),
            SessionData {
                agent: None,
                pending_approvals: Arc::clone(&pending_approvals_arc),
                auto_approve: Arc::clone(&auto_approve_arc),
                current_file: Arc::clone(&current_file_arc),
                cwd: Arc::clone(&cwd_arc),
                cancellation_token: cancel_token.clone(),
            },
        );

        agent
    };
    drop(map);

    // Capture rebuild parameters in case the agent task panics
    let rebuild_provider = Arc::clone(&provider);
    let rebuild_config = config.clone();
    let rebuild_work_dir = work_dir.clone();
    let rebuild_app = app.clone();
    let rebuild_session_id = session_id.clone();
    let rebuild_auto_approve = Arc::clone(&auto_approve_arc);
    let rebuild_pending_approvals = Arc::clone(&pending_approvals_arc);
    let rebuild_current_file = Arc::clone(&current_file_arc);
    let rebuild_cwd = Arc::clone(&cwd_arc);
    let rebuild_augmented_path = augmented_path.clone();
    let rebuild_runtime_venv_dir = runtime_venv_dir.clone();

    let event_prefix = format!("agent://{}", session_id);
    let event_prefix_post = event_prefix.clone();
    let session_id_t = session_id.clone();

    let app_t = app.clone();
    let app_post = app.clone();

    eprintln!("[zcode] start_agent_turn: spawning agent task, event_prefix={event_prefix}");

    tokio::spawn(async move {
        eprintln!("[zcode] agent_task: started, prefix={event_prefix}");
        // Run agent in a sub-task so we can catch JoinError from panics.
        // The sub-task returns (result, agent) so we can always restore the
        // agent to the session afterwards.
        // Track skill names per tool call ID for ToolResult propagation
        let skill_names = Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
            String,
            Option<String>,
        >::new()));
        let run_task = tokio::spawn({
            let skill_names = Arc::clone(&skill_names);
            let inner_token = cancel_token.clone();
            async move {
                let result = agent
                .run(user_message, move |event| {
                    let a = app_t.clone();
                    let pfx = &event_prefix;

                    match event {
                        AgentEvent::MessageUpdate { delta, .. } => {
                            eprintln!("[zcode] agent_task: emitting token '{delta}'");
                            let _ = a
                                .emit(&format!("{pfx}/token"), AgentFrontendEvent::Token { delta });
                        }
                        AgentEvent::ToolStart {
                            tool_call_id,
                            tool_name,
                            arguments,
                        } => {
                            let skill_name = detect_skill_invocation(&tool_name, &arguments);
                            if let Ok(mut map) = skill_names.lock() {
                                map.insert(tool_call_id.clone(), skill_name.clone());
                            }
                            let fe = AgentFrontendEvent::ToolCall {
                                call_id: tool_call_id.clone(),
                                tool_name: tool_name.clone(),
                                arguments: arguments.clone(),
                                skill_name,
                            };
                            eprintln!("[zcode] agent_task: emitting tool-call {}, JSON={}",
                                tool_name,
                                serde_json::to_string(&fe).unwrap_or_default());
                            let _ = a.emit(
                                &format!("{pfx}/tool-call"),
                                fe,
                            );
                        }
                        AgentEvent::ToolEnd {
                            tool_call_id,
                            tool_name,
                            result,
                            is_error,
                        } => {
                            let summary = tool_result_summary(&result.content);
                            let skill_name = skill_names.lock().ok()
                                .and_then(|map| map.get(&tool_call_id).cloned())
                                .flatten();
                            let fe = AgentFrontendEvent::ToolResult {
                                call_id: tool_call_id.clone(),
                                tool_name: tool_name.clone(),
                                is_error,
                                summary: summary.clone(),
                                skill_name,
                            };
                            eprintln!("[zcode] agent_task: emitting tool-result {}, is_error={is_error}, JSON={}",
                                tool_name,
                                serde_json::to_string(&fe).unwrap_or_default());
                            let _ = a.emit(
                                &format!("{pfx}/tool-result"),
                                fe,
                            );
                        }
                        AgentEvent::AgentEnd { error, .. } => {
                            eprintln!("[zcode] agent_task: AgentEnd error={error:?}");
                            if let Some(msg) = error {
                                let _ = a.emit(
                                    &format!("{pfx}/error"),
                                    AgentFrontendEvent::Error { message: msg },
                                );
                            }
                        }
                        AgentEvent::AgentStart { .. } => {
                            eprintln!("[zcode] agent_task: AgentStart");
                        }
                        AgentEvent::TurnStart { turn_index, .. } => {
                            eprintln!("[zcode] agent_task: TurnStart #{turn_index}");
                        }
                        AgentEvent::TurnEnd { .. } => {
                            eprintln!("[zcode] agent_task: TurnEnd");
                        }
                        AgentEvent::MessageStart { .. } => {
                            eprintln!("[zcode] agent_task: MessageStart");
                        }
                        AgentEvent::MessageEnd { .. } => {
                            eprintln!("[zcode] agent_task: MessageEnd");
                        }
                        AgentEvent::CompactionStarted { reason, tokens_before } => {
                            let _ = a.emit(
                                &format!("{pfx}/compaction-started"),
                                AgentFrontendEvent::CompactionStarted {
                                    reason,
                                    tokens_before,
                                },
                            );
                        }
                        AgentEvent::CompactionFinished {
                            tokens_after,
                            summary_len,
                        } => {
                            let _ = a.emit(
                                &format!("{pfx}/compaction-finished"),
                                AgentFrontendEvent::CompactionFinished {
                                    tokens_after,
                                    summary_len,
                                },
                            );
                        }
                        AgentEvent::StuckLoop { tool_name, count } => {
                            let _ = a.emit(
                                &format!("{pfx}/error"),
                                AgentFrontendEvent::Error {
                                    message: format!(
                                        "Stuck loop: '{tool_name}' called {count}x with same args"
                                    ),
                                },
                            );
                        }
                    }
                }, inner_token)
                .await;
                (result, agent)
            }
        });

        match run_task.await {
            Ok((result, agent)) => {
                if cancel_token.is_cancelled() {
                    eprintln!("[zcode] agent_task: cancelled, skipping post-processing for session={session_id_t}");
                    rebuild_pending_approvals.lock().await.clear();
                    return;
                }
                match &result {
                    Ok(msg) => {
                        eprintln!(
                            "[zcode] agent_task: SUCCESS, stop_reason={:?}, tokens in={} out={}",
                            msg.stop_reason, msg.usage.input, msg.usage.output
                        );

                        // Save the final assistant message to disk (best-effort)
                        let assistant_text: String = msg
                            .content
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text(t) => Some(t.text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !assistant_text.is_empty() {
                            let _ = append_session_message(
                                &session_id_t,
                                &ChatMessage {
                                    id: next_msg_id("assistant"),
                                    role: "assistant".to_string(),
                                    content: assistant_text,
                                    input_tokens: Some(msg.usage.input),
                                    output_tokens: Some(msg.usage.output),
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                },
                            );
                        }

                        let _ = app_post.emit(
                            &format!("{}/turn-end", event_prefix_post),
                            AgentFrontendEvent::TurnEnd {
                                stop_reason: format!("{:?}", msg.stop_reason),
                                input_tokens: msg.usage.input,
                                output_tokens: msg.usage.output,
                            },
                        );
                    }
                    Err(e) => {
                        eprintln!("[zcode] agent_task: ERROR from agent: {e}");
                        let _ = app_post.emit(
                            &format!("{}/error", event_prefix_post),
                            AgentFrontendEvent::Error {
                                message: e.to_string(),
                            },
                        );
                        let _ = app_post.emit(
                            &format!("{}/turn-end", event_prefix_post),
                            AgentFrontendEvent::TurnEnd {
                                stop_reason: "Error".to_string(),
                                input_tokens: 0,
                                output_tokens: 0,
                            },
                        );
                    }
                }

                let mut map = sessions.lock().await;
                if let Some(sd) = map.get_mut(&session_id_t) {
                    sd.agent = Some(agent);
                }
            }
            Err(join_err) => {
                eprintln!("[zcode] agent_task: PANIC/join error: {join_err:?}");
                if join_err.is_panic() {
                    let _ = app_post.emit(
                        &format!("{}/error", event_prefix_post),
                        AgentFrontendEvent::Error {
                            message: "Agent task panicked".to_string(),
                        },
                    );
                }
                let _ = app_post.emit(
                    &format!("{}/turn-end", event_prefix_post),
                    AgentFrontendEvent::TurnEnd {
                        stop_reason: "Error".to_string(),
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                );

                // Rebuild agent from captured parameters
                let tool_names_refs: Vec<&str> = allowed_tools_for_rebuild
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let tool_registry = build_guarded_registry(
                    &tool_names_refs,
                    &rebuild_work_dir,
                    rebuild_pending_approvals,
                    rebuild_auto_approve,
                    rebuild_app,
                    &rebuild_session_id,
                    rebuild_current_file,
                    rebuild_cwd,
                    Some(&rebuild_augmented_path),
                    Some(&rebuild_runtime_venv_dir),
                );
                let new_agent = Agent::new(rebuild_provider, tool_registry, rebuild_config);

                let mut map = sessions.lock().await;
                if let Some(sd) = map.get_mut(&session_id_t) {
                    sd.agent = Some(new_agent);
                }
            }
        }
    });

    Ok(())
}

// ============================================================================
// Command: approve_tool_call
// ============================================================================

#[tauri::command]
pub async fn approve_tool_call(
    state: tauri::State<'_, SessionManager>,
    session_id: String,
    call_id: String,
    approved: bool,
) -> Result<(), String> {
    let sessions = state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;

    let mut pending = session.pending_approvals.lock().await;
    if let Some(entry) = pending.remove(&call_id) {
        // Send the decision; ignore error (tool may have timed out)
        let _ = entry.response_tx.send(approved);
        Ok(())
    } else {
        Err(format!("No pending tool call with id: {call_id}"))
    }
}
