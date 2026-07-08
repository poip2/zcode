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
//! - Read-only tools (read, grep, find, ls) execute immediately.

use crate::agent::{Agent, AgentConfig, AgentEvent};
use crate::error::Result as AgentResult;
use crate::model::{ContentBlock, TextContent};
use crate::provider::StreamOptions;
use crate::providers::OpenAIProvider;
use crate::settings;
use crate::skills::{self, Skill};
use crate::tools::{self, Tool, ToolEffects, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, oneshot};

// ============================================================================
// Frontend-facing event types (lightweight, serializable)
// ============================================================================

/// Events emitted from the agent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentFrontendEvent {
    Token { delta: String },
    Thinking { delta: String },
    ToolCall {
        call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        call_id: String,
        tool_name: String,
        is_error: bool,
        summary: String,
    },
    /// A dangerous tool needs user confirmation before execution.
    /// The agent pauses until approve_tool_call is invoked.
    ToolConfirmation {
        call_id: String,
        tool_name: String,
        /// Human-readable summary of what will happen
        summary: String,
        /// Full details (diff, command, file path, etc.)
        details: serde_json::Value,
    },
    TurnEnd {
        stop_reason: String,
        input_tokens: u64,
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
}

/// Global session map, shared between commands and background tasks.
type SessionMap = Arc<Mutex<HashMap<String, SessionData>>>;

/// Tauri managed state wrapper.
pub struct SessionManager {
    pub(crate) sessions: SessionMap,
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
}

impl GuardedTool {
    fn is_dangerous(&self) -> bool {
        let name = self.inner.name();
        name == "write" || name == "edit" || name == "bash"
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
        // Auto-approve if flag is on, or tool is read-only
        if self.auto_approve.load(Ordering::Relaxed) || !self.is_dangerous() {
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
            map.insert(
                call_id.clone(),
                PendingApproval {
                    response_tx: tx,
                },
            );
        }

        // Wait for user decision (with 5-minute timeout as safety)
        let approved = match tokio::time::timeout(
            std::time::Duration::from_secs(300),
            rx,
        )
        .await
        {
            Ok(Ok(true)) => true,
            _ => false,
        };

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
            let content = input
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
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
            let old_text = input
                .get("oldText")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let new_text = input
                .get("newText")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let old_preview = safe_truncate(old_text, 80);
            let new_preview = safe_truncate(new_text, 80);
            format!("Edit `{}`:\n- {} \n+ {}", path, old_preview.replace('\n', "\n- "), new_preview.replace('\n', "\n+ "))
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

fn build_guarded_registry(
    tool_names: &[&str],
    cwd: &std::path::Path,
    pending_approvals: Arc<Mutex<HashMap<String, PendingApproval>>>,
    auto_approve: Arc<AtomicBool>,
    app: AppHandle,
    session_id: &str,
) -> ToolRegistry {
    use crate::tools::{bash::BashTool, edit::EditTool, find::FindTool, grep::GrepTool, ls::LsTool, read::ReadTool, write::WriteTool};

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
        });
        tools.push(guarded);
    }
    ToolRegistry::from_tools(tools)
}

// ============================================================================
// Helper: build system prompt with skills
// ============================================================================

fn build_system_prompt(
    cwd: &std::path::Path,
    active_skills: &[String],
    current_file: Option<&str>,
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
            "The user currently has this file open: {}\n",
            path
        ));
    }
    prompt.push('\n');

    let (all_skills, _diags) = skills::load_skills(cwd, None, &[]);
    let filtered: Vec<Skill> =
        if active_skills.is_empty() || active_skills.iter().any(|s| s == "__all__") {
            all_skills
        } else {
            all_skills
                .into_iter()
                .filter(|s| active_skills.contains(&s.name))
                .collect()
        };

    if !filtered.is_empty() {
        prompt.push_str(&skills::format_skills_for_prompt(&filtered));
    }

    prompt.push_str("\n\nAlways respond in the same language as the user's message.\n");
    prompt
}

// ============================================================================
// Helper: extract text summary from tool result
// ============================================================================

fn safe_truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        let end = if s.is_char_boundary(max_bytes) {
            max_bytes
        } else {
            (0..max_bytes).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0)
        };
        format!("{}...", &s[..end])
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
pub async fn start_agent_turn(
    app: AppHandle,
    state: tauri::State<'_, SessionManager>,
    session_id: String,
    user_message: String,
    allowed_tools: Vec<String>,
    active_skills: Vec<String>,
    base_url: String,
    model: String,
    provider_name: Option<String>,
    current_file: Option<String>,
    cwd: Option<String>,
    auto_approve_writes: Option<bool>,
) -> Result<(), String> {
    if user_message.trim().is_empty() {
        return Err("User message cannot be empty".to_string());
    }
    if base_url.is_empty() {
        return Err("No Base URL configured".to_string());
    }
    if model.is_empty() {
        return Err("No model configured".to_string());
    }

    let name = provider_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "openai".to_string());

    let api_key = settings::get_api_key()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No API key configured".to_string())?;

    let work_dir = if let Some(ref d) = cwd {
        PathBuf::from(d)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    // Build system prompt
    let system_prompt = build_system_prompt(&work_dir, &active_skills, current_file.as_deref());

    // Build provider
    let provider = OpenAIProvider::new(&name, &model, Some(&api_key), Some(&base_url))
        .map_err(|e| e.to_string())?;
    let provider: Arc<dyn crate::provider::Provider> = Arc::new(provider);

    // Agent config
    let config = AgentConfig {
        system_prompt: Some(system_prompt),
        max_tool_iterations: 50,
        stream_options: StreamOptions {
            session_id: Some(session_id.clone()),
            ..Default::default()
        },
    };

    // Get or create session
    let sessions = state.sessions.clone();
    let mut map = sessions.lock().await;

    let mut agent = if let Some(sd) = map.get_mut(&session_id) {
        sd.auto_approve.store(auto_approve_writes.unwrap_or(false), Ordering::Relaxed);
        sd.agent.take()
            .ok_or_else(|| "Agent is already running for this session".to_string())?
    } else {
        let auto_approve = Arc::new(AtomicBool::new(auto_approve_writes.unwrap_or(false)));
        let pending_approvals = Arc::new(Mutex::new(HashMap::new()));

        // Build guarded tool registry
        let tool_names: Vec<&str> = if allowed_tools.is_empty() {
            vec!["read", "write", "edit", "bash", "grep", "find", "ls"]
        } else {
            allowed_tools.iter().map(|s| s.as_str()).collect()
        };

        let tool_registry = build_guarded_registry(
            &tool_names,
            &work_dir,
            Arc::clone(&pending_approvals),
            Arc::clone(&auto_approve),
            app.clone(),
            &session_id,
        );

        let agent = Agent::new(Arc::clone(&provider), tool_registry, config.clone());

        map.insert(session_id.clone(), SessionData {
            agent: None,
            pending_approvals,
            auto_approve,
        });

        agent
    };
    drop(map);

    let event_prefix = format!("agent://{}", session_id);
    let event_prefix_post = event_prefix.clone();
    let session_id_t = session_id.clone();

    let app_t = app.clone();
    let app_post = app.clone();

    tokio::spawn(async move {
        let result = agent
            .run(user_message, move |event| {
                let a = app_t.clone();
                let pfx = &event_prefix;

                match event {
                    AgentEvent::MessageUpdate { delta, .. } => {
                        let _ = a.emit(
                            &format!("{pfx}/token"),
                            AgentFrontendEvent::Token { delta },
                        );
                    }
                    AgentEvent::ToolStart {
                        tool_call_id,
                        tool_name,
                        arguments,
                    } => {
                        let _ = a.emit(
                            &format!("{pfx}/tool-call"),
                            AgentFrontendEvent::ToolCall {
                                call_id: tool_call_id,
                                tool_name,
                                arguments,
                            },
                        );
                    }
                    AgentEvent::ToolEnd {
                        tool_call_id,
                        tool_name,
                        result,
                        is_error,
                    } => {
                        let summary = tool_result_summary(&result.content);
                        let _ = a.emit(
                            &format!("{pfx}/tool-result"),
                            AgentFrontendEvent::ToolResult {
                                call_id: tool_call_id,
                                tool_name,
                                is_error,
                                summary,
                            },
                        );
                    }
                    AgentEvent::AgentEnd { error, .. } => {
                        if let Some(msg) = error {
                            let _ = a.emit(
                                &format!("{pfx}/error"),
                                AgentFrontendEvent::Error { message: msg },
                            );
                        }
                    }
                    AgentEvent::AgentStart { .. }
                    | AgentEvent::TurnStart { .. }
                    | AgentEvent::TurnEnd { .. }
                    | AgentEvent::MessageStart { .. }
                    | AgentEvent::MessageEnd { .. } => {}
                }
            })
            .await;

        match &result {
            Ok(msg) => {
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
