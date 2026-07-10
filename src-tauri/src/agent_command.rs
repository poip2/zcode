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
//! - Dangerous tools (write, edit, bash) wait for user approval via oneshot
//!   channels stored in the session's approval map.
//! - Write/edit operations targeting the user's currently-open file skip the
//!   confirmation dialog (smart auto-approve). The session's `current_file`
//!   and `cwd` are updated each turn from the frontend.
//! - Read-only tools (read, grep, find, ls) execute immediately.

use crate::agent::{Agent, AgentConfig, AgentEvent};
use crate::error::Result as AgentResult;
use crate::model::{ContentBlock, TextContent};
use crate::provider::StreamOptions;
use crate::settings;
use crate::skills;
use crate::tools::{self, Tool, ToolEffects, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};

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
}

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
        name == "write" || name == "edit" || name == "bash"
    }

    /// Returns true when the tool's target path matches the currently-open file.
    /// Only applies to write/edit (not bash). Relative paths are resolved against `cwd`.
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
        match (cur.canonicalize(), absolute.canonicalize()) {
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
        "bash" => {
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
        "bash" => serde_json::json!({
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
) -> ToolRegistry {
    use crate::tools::{
        bash::BashTool, edit::EditTool, find::FindTool, grep::GrepTool, ls::LsTool, read::ReadTool,
        write::WriteTool,
    };

    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    for name in tool_names {
        let tool: Box<dyn Tool> = match *name {
            "read" => Box::new(ReadTool::new(cwd)),
            "bash" => Box::new(BashTool::new(cwd)),
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
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
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
) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are an AI assistant embedded in zcode, a Markdown editor. \
         You help the user read, write, and edit files in their project. \
         You have access to tools for reading files, writing files, editing files, \
         running shell commands, searching with grep, finding files, and listing directories.\n\n",
    );

    prompt.push_str(&format!("Working directory: {}\n", cwd.display()));

    if let Some(path) = current_file {
        prompt.push_str(&format!(
            "The user currently has this file open: {path}\n\
             You may freely edit this file without waiting for approval.\n\
             Editing any OTHER file requires explicit user confirmation.\n"
        ));
    }
    prompt.push('\n');

    // Load skills from project dir + user config dir, filter by persisted state
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

    prompt.push_str("\n\nAlways respond in the same language as the user's message.\n");
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

// ============================================================================
// Command: start_agent_turn
// ============================================================================

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_agent_turn(
    app: AppHandle,
    state: tauri::State<'_, SessionManager>,
    session_id: String,
    user_message: String,
    allowed_tools: Vec<String>,
    base_url: String,
    model: String,
    provider_name: Option<String>,
    current_file: Option<String>,
    cwd: Option<String>,
    auto_approve_writes: Option<bool>,
) -> Result<(), String> {
    eprintln!("[zcode] start_agent_turn: session={session_id}, base_url={base_url}, model={model}, msg_len={}", user_message.len());
    eprintln!("[zcode] start_agent_turn: tools={allowed_tools:?}, auto_approve={auto_approve_writes:?}");

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

    // Build system prompt
    let user_config_dir = dirs::config_dir().map(|d| d.join("zcode"));
    let system_prompt =
        build_system_prompt(&work_dir, current_file.as_deref(), user_config_dir.as_deref());
    eprintln!(
        "[zcode] start_agent_turn: system_prompt len={}, cwd={}",
        system_prompt.len(),
        work_dir.display()
    );

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
        vec!["read", "write", "edit", "bash", "grep", "find", "ls"]
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

    let mut agent = if let Some(sd) = map.get_mut(&session_id) {
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
        reused
    } else {
        auto_approve_arc = Arc::new(AtomicBool::new(auto_approve_writes.unwrap_or(false)));
        pending_approvals_arc = Arc::new(Mutex::new(HashMap::new()));
        current_file_arc = Arc::new(std::sync::Mutex::new(current_file.clone()));
        cwd_arc = Arc::new(std::sync::Mutex::new(work_dir.clone()));

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
        );

        let agent = Agent::new(Arc::clone(&provider), tool_registry, config.clone());

        map.insert(
            session_id.clone(),
            SessionData {
                agent: None,
                pending_approvals: Arc::clone(&pending_approvals_arc),
                auto_approve: Arc::clone(&auto_approve_arc),
                current_file: Arc::clone(&current_file_arc),
                cwd: Arc::clone(&cwd_arc),
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
        let skill_names = Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::<String, Option<String>>::new(),
        ));
        let run_task = tokio::spawn({
            let skill_names = Arc::clone(&skill_names);
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
                    }
                })
                .await;
            (result, agent)
        }
        });

        match run_task.await {
            Ok((result, agent)) => {
                match &result {
                    Ok(msg) => {
                        eprintln!(
                            "[zcode] agent_task: SUCCESS, stop_reason={:?}, tokens in={} out={}",
                            msg.stop_reason, msg.usage.input, msg.usage.output
                        );
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


