//! Agent runtime — core orchestration loop.
//!
//! Adapted from pi-agent-rust (src/agent.rs).
//! Coordinates between Provider → Tools → Message history.
//!
//! Main loop:
//! 1. Receive user input
//! 2. Build context (system prompt + history + tools)
//! 3. Stream completion from provider
//! 4. If tool calls: execute tools, append results, goto 3
//! 5. If done: return final message

use crate::compaction::{self, CompactionSettings};
use crate::error::Result;
use crate::model::{
    AssistantMessage, ContentBlock, CustomMessage, Message, StopReason, StreamEvent, TextContent,
    ToolCall, ToolResultMessage, Usage, UserContent, UserMessage,
};
use crate::provider::{Context, Provider, StreamOptions, ToolDef};
use crate::tools::{ToolOutput, ToolRegistry};
use futures::StreamExt;
use serde::Serialize;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

// ============================================================================
// Agent Events
// ============================================================================

/// Events emitted by the agent during execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    /// Agent lifecycle start.
    AgentStart { session_id: String },
    /// Agent lifecycle end.
    AgentEnd {
        session_id: String,
        messages: Vec<Message>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Turn start (assistant response begins).
    TurnStart {
        session_id: String,
        turn_index: usize,
    },
    /// Turn end with tool results.
    TurnEnd {
        session_id: String,
        turn_index: usize,
        message: Message,
        tool_results: Vec<Message>,
    },
    /// A message was added to history.
    MessageStart { message: Message },
    /// Streaming update to assistant message.
    MessageUpdate { message: Message, delta: String },
    /// Message complete.
    MessageEnd { message: Message },
    /// Tool execution started.
    ToolStart {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    /// Tool execution completed.
    ToolEnd {
        tool_call_id: String,
        tool_name: String,
        result: ToolOutput,
        is_error: bool,
    },
    /// Context compaction started (inline, blocks the current turn).
    CompactionStarted { reason: String, tokens_before: u64 },
    /// Context compaction finished.
    CompactionFinished {
        tokens_after: u64,
        summary_len: usize,
    },
    /// Agent was stopped due to stuck tool call loop.
    StuckLoop { tool_name: String, count: usize },
}

// ============================================================================
// Agent Config
// ============================================================================

/// Configuration for the agent.
#[derive(Clone)]
pub struct AgentConfig {
    /// System prompt to use for all requests.
    pub system_prompt: Option<String>,

    /// Maximum tool call iterations before stopping.
    pub max_tool_iterations: usize,

    /// Default stream options.
    pub stream_options: StreamOptions,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: None,
            max_tool_iterations: 50,
            stream_options: StreamOptions::default(),
        }
    }
}

// ============================================================================
// Agent
// ============================================================================

/// The agent runtime that orchestrates LLM calls and tool execution.
pub struct Agent {
    /// The LLM provider.
    provider: Arc<dyn Provider>,

    /// Tool registry.
    tools: ToolRegistry,

    /// Agent configuration.
    config: AgentConfig,

    /// Message history.
    messages: Vec<Message>,

    /// Cached tool definitions.
    cached_tool_defs: Option<Vec<ToolDef>>,

    /// Auto-compaction settings (None = disabled).
    compaction_settings: Option<CompactionSettings>,

    /// Previous compaction summary for iterative summarization.
    previous_summary: Option<String>,

    /// Recent tool calls for stuck-loop detection.
    recent_tool_calls: Vec<(String, serde_json::Value)>,

    /// Turn index of the most recent compaction (for cooldown).
    last_compaction_turn: Option<usize>,

    /// Consecutive compactions that failed to bring tokens below threshold.
    consecutive_compaction_failures: u32,

    /// Cached system prompt token count. Re-computed when the system prompt changes.
    cached_system_prompt_tokens: u64,

    /// Indices in self.messages of ToolResults that still contain real
    /// ContentBlock::Image blocks (not yet aged out into text placeholders).
    /// Cleared on compaction (which reorders messages).
    pending_image_indices: Vec<usize>,
}

impl Agent {
    /// Create a new agent.
    pub fn new(provider: Arc<dyn Provider>, tools: ToolRegistry, config: AgentConfig) -> Self {
        let system_prompt_tokens =
            Self::compute_system_prompt_tokens(config.system_prompt.as_deref());
        Self {
            provider,
            tools,
            config,
            messages: Vec::new(),
            cached_tool_defs: None,
            compaction_settings: Some(CompactionSettings::default()),
            previous_summary: None,
            recent_tool_calls: Vec::new(),
            last_compaction_turn: None,
            consecutive_compaction_failures: 0,
            cached_system_prompt_tokens: system_prompt_tokens,
            pending_image_indices: Vec::new(),
        }
    }

    /// Get the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Clear message history and all associated compaction/loop-detection state.
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.pending_image_indices.clear();
        self.previous_summary = None;
        self.last_compaction_turn = None;
        self.consecutive_compaction_failures = 0;
        self.recent_tool_calls.clear();
    }

    /// Add a message to history.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Replace message history.
    pub fn replace_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.pending_image_indices.clear();
    }

    /// Seed the agent's history with existing messages (e.g. loaded from disk).
    /// These are added directly to self.messages as existing context —
    /// they are NOT treated as new prompts that trigger a tool loop.
    pub fn seed_history(&mut self, messages: Vec<Message>) {
        self.messages.extend(messages);
    }

    pub fn provider(&self) -> Arc<dyn Provider> {
        Arc::clone(&self.provider)
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt.as_deref()
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.cached_system_prompt_tokens = Self::compute_system_prompt_tokens(prompt.as_deref());
        self.config.system_prompt = prompt;
    }

    fn compute_system_prompt_tokens(system_prompt: Option<&str>) -> u64 {
        match system_prompt {
            Some(p) if !p.is_empty() => compaction::estimate_text_tokens(p),
            _ => 0,
        }
    }

    /// Configure auto-compaction. Pass `None` to disable.
    pub fn set_compaction_settings(&mut self, settings: Option<CompactionSettings>) {
        self.compaction_settings = settings;
    }

    /// Get the current compaction summary, if any.
    pub fn previous_summary(&self) -> Option<&str> {
        self.previous_summary.as_deref()
    }

    /// Ensure tool definitions are cached.
    fn ensure_tool_defs(&mut self) {
        if self.cached_tool_defs.is_none() {
            let defs: Vec<ToolDef> = self
                .tools
                .tools()
                .iter()
                .map(|t| ToolDef {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.parameters(),
                })
                .collect();
            self.cached_tool_defs = Some(defs);
        }
    }

    /// Build a context for a provider request (immutable borrow).
    fn build_context(&self) -> Context<'_> {
        Context {
            system_prompt: self.config.system_prompt.as_deref(),
            messages: &self.messages,
            tools: self.cached_tool_defs.as_deref().unwrap_or(&[]),
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Run the agent with a text prompt.
    pub async fn run(
        &mut self,
        user_input: impl Into<String>,
        on_event: impl Fn(AgentEvent) + Send + Sync + 'static,
        cancel_token: CancellationToken,
    ) -> Result<AssistantMessage> {
        let msg = Message::User(UserMessage {
            content: UserContent::Text(user_input.into()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
        self.run_loop(vec![msg], Arc::new(on_event), cancel_token)
            .await
    }

    /// Run the agent with a pre-built message list.
    pub async fn run_with_messages(
        &mut self,
        messages: Vec<Message>,
        on_event: impl Fn(AgentEvent) + Send + Sync + 'static,
        cancel_token: CancellationToken,
    ) -> Result<AssistantMessage> {
        self.run_loop(messages, Arc::new(on_event), cancel_token)
            .await
    }

    // ========================================================================
    // Core Loop
    // ========================================================================

    /// Replace any ContentBlock::Image in content with text placeholders.
    fn replace_images_with_placeholders(content: &[ContentBlock]) -> Vec<ContentBlock> {
        content
            .iter()
            .map(|block| match block {
                ContentBlock::Image(img) => {
                    let size_kb = img.data.len() / 1024;
                    ContentBlock::Text(TextContent::new(format!(
                        "[已读取图片: {}, {}KB]",
                        img.mime_type, size_kb
                    )))
                }
                other => other.clone(),
            })
            .collect()
    }

    /// Replace any ContentBlock::Image entries in a ToolResult message with text placeholders.
    fn age_out_image(msg: &mut Message) {
        let tr = match msg {
            Message::ToolResult(tr) => tr,
            _ => return,
        };
        let has_image = tr
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::Image(_)));
        if !has_image {
            return;
        }
        let new_tr = ToolResultMessage {
            content: Self::replace_images_with_placeholders(&tr.content),
            tool_call_id: tr.tool_call_id.clone(),
            tool_name: tr.tool_name.clone(),
            details: tr.details.clone(),
            is_error: tr.is_error,
            timestamp: tr.timestamp,
        };
        *msg = Message::tool_result(new_tr);
    }

    fn result_has_image(content: &[ContentBlock]) -> bool {
        content.iter().any(|b| matches!(b, ContentBlock::Image(_)))
    }

    /// Create a placeholder assistant message for early cancellation returns.
    fn cancelled_message(model: &str) -> AssistantMessage {
        AssistantMessage {
            content: vec![ContentBlock::Text(TextContent::new(
                "[Session cancelled]".to_string(),
            ))],
            api: String::new(),
            provider: String::new(),
            model: model.to_string(),
            usage: Usage::default(),
            stop_reason: StopReason::Error,
            error_message: Some("Session cancelled by user".to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    async fn run_loop(
        &mut self,
        prompts: Vec<Message>,
        on_event: Arc<dyn Fn(AgentEvent) + Send + Sync + 'static>,
        cancel_token: CancellationToken,
    ) -> Result<AssistantMessage> {
        let session_id = self
            .config
            .stream_options
            .session_id
            .clone()
            .unwrap_or_default();

        let mut iterations = 0usize;
        let mut turn_index: usize = 0;
        let mut new_messages: Vec<Message> = Vec::new();
        let mut last_assistant: Option<AssistantMessage> = None;

        // AgentStart
        eprintln!(
            "[zcode] agent::run_loop: starting, session={session_id}, provider={}, history_len={}",
            self.provider.name(),
            self.messages.len()
        );
        on_event(AgentEvent::AgentStart {
            session_id: session_id.clone(),
        });

        // Add prompts to history
        for prompt in prompts {
            self.messages.push(prompt.clone());
            on_event(AgentEvent::MessageStart {
                message: prompt.clone(),
            });
            on_event(AgentEvent::MessageEnd {
                message: prompt.clone(),
            });
            new_messages.push(prompt);
        }

        let is_cancelled = || cancel_token.is_cancelled();

        let mut has_more_tool_calls = true;

        while has_more_tool_calls {
            if is_cancelled() {
                eprintln!("[zcode] agent::run_loop: cancelled before turn");
                let model = self.provider.model_id().to_string();
                let partial = last_assistant.unwrap_or_else(|| Self::cancelled_message(&model));
                on_event(AgentEvent::AgentEnd {
                    session_id: session_id.clone(),
                    messages: new_messages,
                    error: Some("Session cancelled".to_string()),
                });
                return Ok(partial);
            }
            let current_turn = turn_index;
            on_event(AgentEvent::TurnStart {
                session_id: session_id.clone(),
                turn_index: current_turn,
            });

            // 1. Check if we need to compact context before sending
            if let Some(ref settings) = self.compaction_settings {
                // Rate-limit: skip compaction if we just compacted recently
                let in_cooldown = self.last_compaction_turn.is_some_and(|t| {
                    current_turn.saturating_sub(t) < settings.compaction_cooldown_turns
                });

                if self.consecutive_compaction_failures as usize
                    >= settings.max_consecutive_compactions
                {
                    // Thrashing kill-switch: compaction has repeatedly failed to reduce
                    // context below threshold, so stop trying.
                    eprintln!(
                        "[zcode] agent::run_loop: thrashing kill-switch: {failures} consecutive \
                         compactions failed to stay under threshold, disabling compaction",
                        failures = self.consecutive_compaction_failures
                    );
                } else if in_cooldown {
                    eprintln!(
                        "[zcode] agent::run_loop: skipping compaction (cooldown, last turn {last:?})",
                        last = self.last_compaction_turn
                    );
                } else {
                    if is_cancelled() {
                        eprintln!("[zcode] agent::run_loop: cancelled before compaction");
                        let model = self.provider.model_id().to_string();
                        let partial = last_assistant
                            .clone()
                            .unwrap_or_else(|| Self::cancelled_message(&model));
                        on_event(AgentEvent::AgentEnd {
                            session_id: session_id.clone(),
                            messages: new_messages,
                            error: Some("Session cancelled".to_string()),
                        });
                        return Ok(partial);
                    }
                    let estimated = compaction::estimate_total_tokens(
                        &self.messages,
                        self.cached_system_prompt_tokens,
                    );
                    if compaction::should_compact(estimated, settings) {
                        on_event(AgentEvent::CompactionStarted {
                            reason: format!(
                                "context ({estimated} tokens) exceeds threshold ({})",
                                settings.trigger_threshold()
                            ),
                            tokens_before: estimated,
                        });

                        let provider = Arc::clone(&self.provider);
                        let compact_result = {
                            let token = cancel_token.clone();
                            tokio::select! {
                                _ = token.cancelled() => {
                                    eprintln!("[zcode] agent::run_loop: cancelled during compaction");
                                    let model = self.provider.model_id().to_string();
                                    let partial = last_assistant.clone().unwrap_or_else(|| Self::cancelled_message(&model));
                                    on_event(AgentEvent::AgentEnd {
                                        session_id: session_id.clone(),
                                        messages: new_messages,
                                        error: Some("Session cancelled".to_string()),
                                    });
                                    return Ok(partial);
                                }
                                r = compaction::maybe_compact(
                                    &self.messages,
                                    self.previous_summary.as_deref(),
                                    provider,
                                    settings,
                                    Some(estimated),
                                    self.cached_system_prompt_tokens,
                                ) => r,
                            }
                        };
                        match compact_result {
                            Ok(Some(result)) => {
                                eprintln!(
                                    "[zcode] agent::run_loop: compaction applied, {} msgs → {} keyst",
                                    result.messages_summarized,
                                    result.messages_kept
                                );
                                self.last_compaction_turn = Some(current_turn);
                                for idx in self.pending_image_indices.drain(..) {
                                    if let Some(msg) = self.messages.get_mut(idx) {
                                        Self::age_out_image(msg);
                                    }
                                }
                                compaction::apply_compaction(&mut self.messages, &result);
                                self.previous_summary = Some(result.summary.clone());

                                // Track whether compaction actually reduced tokens below threshold.
                                // If consecutive compactions fail to converge, we'll hit the
                                // kill-switch above.
                                if result.tokens_after >= settings.trigger_threshold() {
                                    self.consecutive_compaction_failures =
                                        self.consecutive_compaction_failures.saturating_add(1);
                                } else {
                                    self.consecutive_compaction_failures = 0;
                                }

                                on_event(AgentEvent::CompactionFinished {
                                    tokens_after: result.tokens_after,
                                    summary_len: result.summary.len(),
                                });
                            }
                            Ok(None) => {
                                // No compaction needed
                            }
                            Err(e) => {
                                eprintln!("[zcode] agent::run_loop: compaction FAILED: {e}");
                                // Continue without compaction — better than losing the session
                            }
                        }
                    }
                }
            }

            // 2. Stream assistant response
            self.ensure_tool_defs();
            let context = self.build_context();
            let stream_options = self.config.stream_options.clone();

            let stream_result = {
                let token = cancel_token.clone();
                tokio::select! {
                    _ = token.cancelled() => {
                        eprintln!("[zcode] agent::run_loop: cancelled before LLM call");
                        let model = self.provider.model_id().to_string();
                        let partial = last_assistant.clone().unwrap_or_else(|| Self::cancelled_message(&model));
                        on_event(AgentEvent::AgentEnd {
                            session_id: session_id.clone(),
                            messages: new_messages,
                            error: Some("Session cancelled".to_string()),
                        });
                        return Ok(partial);
                    }
                    r = self.provider.stream(&context, &stream_options) => r,
                }
            };

            let mut stream = match stream_result {
                Ok(s) => {
                    eprintln!("[zcode] agent::run_loop: turn #{current_turn} provider.stream() started OK");
                    s
                }
                Err(first_err) => {
                    if !self.pending_image_indices.is_empty() {
                        eprintln!(
                            "[zcode] agent::run_loop: turn #{current_turn} stream failed, retrying after aging out {} image(s): {first_err}",
                            self.pending_image_indices.len()
                        );
                        for idx in self.pending_image_indices.drain(..) {
                            if let Some(msg) = self.messages.get_mut(idx) {
                                Self::age_out_image(msg);
                            }
                        }
                        let retry_context = self.build_context();
                        match self.provider.stream(&retry_context, &stream_options).await {
                            Ok(s) => {
                                eprintln!("[zcode] agent::run_loop: turn #{current_turn} retry after image age-out succeeded");
                                s
                            }
                            Err(retry_err) => {
                                eprintln!("[zcode] agent::run_loop: turn #{current_turn} retry also FAILED: {retry_err}");
                                on_event(AgentEvent::AgentEnd {
                                    session_id: session_id.clone(),
                                    messages: new_messages,
                                    error: Some(retry_err.to_string()),
                                });
                                return Err(retry_err);
                            }
                        }
                    } else {
                        eprintln!("[zcode] agent::run_loop: turn #{current_turn} provider.stream() FAILED: {first_err}");
                        on_event(AgentEvent::AgentEnd {
                            session_id: session_id.clone(),
                            messages: new_messages,
                            error: Some(first_err.to_string()),
                        });
                        return Err(first_err);
                    }
                }
            };

            let mut assistant_arc: Option<Arc<AssistantMessage>> = None;
            let mut error_occurred = false;

            while let Some(event) = stream.next().await {
                if is_cancelled() {
                    eprintln!("[zcode] agent::run_loop: cancelled during stream");
                    // Use whatever partial text was accumulated
                    if let Some(ref msg) = assistant_arc {
                        on_event(AgentEvent::MessageEnd {
                            message: Message::Assistant(Arc::clone(msg)),
                        });
                    }
                    let model = self.provider.model_id().to_string();
                    let partial = assistant_arc
                        .map(Arc::unwrap_or_clone)
                        .or_else(|| last_assistant.clone())
                        .unwrap_or_else(|| Self::cancelled_message(&model));
                    on_event(AgentEvent::AgentEnd {
                        session_id: session_id.clone(),
                        messages: new_messages,
                        error: Some("Session cancelled".to_string()),
                    });
                    return Ok(partial);
                }
                match event {
                    Ok(StreamEvent::TextDelta { delta, .. }) => {
                        // Accumulate into in-progress assistant message
                        if let Some(ref msg_arc) = assistant_arc {
                            let mut msg_clone = (**msg_arc).clone();
                            if msg_clone.content.is_empty()
                                || !matches!(msg_clone.content.last(), Some(ContentBlock::Text(_)))
                            {
                                msg_clone
                                    .content
                                    .push(ContentBlock::Text(TextContent::new(delta.clone())));
                            } else if let Some(ContentBlock::Text(tc)) =
                                msg_clone.content.last_mut()
                            {
                                tc.text.push_str(&delta);
                            }
                            on_event(AgentEvent::MessageUpdate {
                                message: Message::Assistant(Arc::new(msg_clone.clone())),
                                delta,
                            });
                            // Re-create Arc with accumulated state
                            assistant_arc = Some(Arc::new(msg_clone));
                        } else {
                            // First text chunk: create initial assistant message
                            eprintln!("[zcode] agent::run_loop: first text delta '{delta}'");
                            let msg = AssistantMessage {
                                content: vec![ContentBlock::Text(TextContent::new(delta.clone()))],
                                api: self.provider.api().to_string(),
                                provider: self.provider.name().to_string(),
                                model: self.provider.model_id().to_string(),
                                usage: Usage::default(),
                                stop_reason: StopReason::Stop,
                                error_message: None,
                                timestamp: chrono::Utc::now().timestamp_millis(),
                            };
                            assistant_arc = Some(Arc::new(msg.clone()));
                            on_event(AgentEvent::MessageUpdate {
                                message: Message::Assistant(Arc::new(msg)),
                                delta,
                            });
                        }
                    }
                    Ok(StreamEvent::Done { reason: _, message }) => {
                        eprintln!(
                            "[zcode] agent::run_loop: Done, content_blocks={}, output_tokens={}",
                            message.content.len(),
                            message.usage.output
                        );
                        assistant_arc = Some(Arc::new(message));
                        // Signal done
                        if let Some(ref msg) = assistant_arc {
                            on_event(AgentEvent::MessageEnd {
                                message: Message::Assistant(Arc::clone(msg)),
                            });
                        }
                        break;
                    }
                    Ok(StreamEvent::Error { error, .. }) => {
                        eprintln!(
                            "[zcode] agent::run_loop: StreamEvent::Error {:?}",
                            error.error_message
                        );
                        assistant_arc = Some(Arc::new(error));
                        error_occurred = true;
                        break;
                    }
                    Ok(StreamEvent::Start { .. }) => {
                        eprintln!("[zcode] agent::run_loop: StreamEvent::Start");
                        // Create empty assistant message
                        assistant_arc = Some(Arc::new(AssistantMessage {
                            content: Vec::new(),
                            api: self.provider.api().to_string(),
                            provider: self.provider.name().to_string(),
                            model: self.provider.model_id().to_string(),
                            usage: Usage::default(),
                            stop_reason: StopReason::Stop,
                            error_message: None,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        }));
                    }
                    Err(e) => {
                        eprintln!("[zcode] agent::run_loop: stream error: {e}");
                        on_event(AgentEvent::AgentEnd {
                            session_id: session_id.clone(),
                            messages: new_messages.clone(),
                            error: Some(e.to_string()),
                        });
                        return Err(e);
                    }
                    _ => {} // Ignore other events for now
                }
            }

            // Get final assistant message
            let Some(assistant_msg_arc) = assistant_arc else {
                for idx in self.pending_image_indices.drain(..) {
                    if let Some(msg) = self.messages.get_mut(idx) {
                        Self::age_out_image(msg);
                    }
                }
                let msg = AssistantMessage {
                    content: vec![],
                    api: self.provider.api().to_string(),
                    provider: self.provider.name().to_string(),
                    model: self.provider.model_id().to_string(),
                    usage: Usage::default(),
                    stop_reason: StopReason::Error,
                    error_message: Some("No response from provider".into()),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                on_event(AgentEvent::AgentEnd {
                    session_id: session_id.clone(),
                    messages: new_messages,
                    error: Some("No response from provider".into()),
                });
                return Ok(msg);
            };

            let assistant_msg = Arc::unwrap_or_clone(assistant_msg_arc);
            last_assistant = Some(assistant_msg.clone());

            let event_msg = Message::assistant(assistant_msg.clone());
            self.messages.push(event_msg.clone());
            new_messages.push(event_msg.clone());

            // Age out any pending image that was sent in this turn's stream call.
            // The real base64 data was included in the request we just made;
            // future turns only need the text placeholder.
            for idx in self.pending_image_indices.drain(..) {
                if let Some(msg) = self.messages.get_mut(idx) {
                    Self::age_out_image(msg);
                }
            }

            if error_occurred {
                on_event(AgentEvent::AgentEnd {
                    session_id: session_id.clone(),
                    messages: new_messages,
                    error: assistant_msg.error_message.clone(),
                });
                return Ok(assistant_msg);
            }

            // 2. Extract tool calls
            let tool_calls: Vec<&ToolCall> = assistant_msg
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolCall(tc) => Some(tc),
                    _ => None,
                })
                .collect();

            has_more_tool_calls = !tool_calls.is_empty();
            let mut tool_messages: Vec<Message> = Vec::new();

            if has_more_tool_calls {
                iterations += 1;

                // Stuck-loop detection: same tool + same args 3x in a row
                if tool_calls.len() == 1 {
                    let tc = tool_calls[0];
                    let key = (tc.name.clone(), tc.arguments.clone());
                    self.recent_tool_calls.push(key);
                    let window = 3usize;
                    let excess = self.recent_tool_calls.len().saturating_sub(window);
                    self.recent_tool_calls.drain(..excess);
                    if self.recent_tool_calls.len() >= window {
                        let tail = &self.recent_tool_calls[self.recent_tool_calls.len() - window..];
                        if tail.iter().all(|t| t.0 == tail[0].0 && t.1 == tail[0].1) {
                            let err_msg = format!(
                                "Detected stuck loop: '{}' called {} times with same arguments",
                                tail[0].0, window
                            );
                            on_event(AgentEvent::StuckLoop {
                                tool_name: tail[0].0.clone(),
                                count: window,
                            });
                            let mut stop = assistant_msg.clone();
                            stop.stop_reason = StopReason::Error;
                            stop.error_message = Some(err_msg.clone());
                            on_event(AgentEvent::AgentEnd {
                                session_id: session_id.clone(),
                                messages: new_messages,
                                error: Some(err_msg),
                            });
                            return Ok(stop);
                        }
                    }
                } else {
                    // Multiple tool calls in one message — clear tracking
                    self.recent_tool_calls.clear();
                }

                if iterations > self.config.max_tool_iterations {
                    let err_msg = format!(
                        "Maximum tool iterations ({}) exceeded",
                        self.config.max_tool_iterations
                    );
                    let mut stop = assistant_msg;
                    stop.stop_reason = StopReason::Error;
                    stop.error_message = Some(err_msg.clone());
                    on_event(AgentEvent::AgentEnd {
                        session_id: session_id.clone(),
                        messages: new_messages,
                        error: Some(err_msg),
                    });
                    return Ok(stop);
                }

                // 3. Execute tool calls
                if is_cancelled() {
                    eprintln!("[zcode] agent::run_loop: cancelled before tool execution");
                    let model = self.provider.model_id().to_string();
                    let partial = last_assistant.unwrap_or_else(|| Self::cancelled_message(&model));
                    on_event(AgentEvent::AgentEnd {
                        session_id: session_id.clone(),
                        messages: new_messages,
                        error: Some("Session cancelled".to_string()),
                    });
                    return Ok(partial);
                }
                for tc in &tool_calls {
                    on_event(AgentEvent::ToolStart {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    });

                    let result = match self.tools.get(&tc.name) {
                        Some(tool) => {
                            let token = cancel_token.clone();
                            tokio::select! {
                                _ = token.cancelled() => ToolOutput {
                                    content: vec![ContentBlock::Text(TextContent::new(
                                        "Tool execution cancelled".to_string(),
                                    ))],
                                    details: None,
                                    is_error: true,
                                },
                                r = tool.execute(&tc.id, tc.arguments.clone(), None) => {
                                    r.unwrap_or_else(|e| ToolOutput {
                                        content: vec![ContentBlock::Text(TextContent::new(format!(
                                            "Tool error: {e}"
                                        )))],
                                        details: None,
                                        is_error: true,
                                    })
                                }
                            }
                        }
                        None => ToolOutput {
                            content: vec![ContentBlock::Text(TextContent::new(format!(
                                "Unknown tool: {}",
                                tc.name
                            )))],
                            details: None,
                            is_error: true,
                        },
                    };

                    let is_error = result.is_error;
                    on_event(AgentEvent::ToolEnd {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        result: result.clone(),
                        is_error,
                    });

                    // Push tool result with real Image blocks to history.
                    // The image will be sent to the provider next turn,
                    // then aged out into a text placeholder immediately after.
                    let has_image = Self::result_has_image(&result.content);

                    let history_msg = Message::tool_result(ToolResultMessage {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        content: result.content.clone(),
                        details: result.details.clone(),
                        is_error,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    });

                    let idx = self.messages.len();
                    self.messages.push(history_msg);
                    if has_image {
                        self.pending_image_indices.push(idx);
                    }

                    // TurnEnd event uses text placeholder (frontend doesn't need base64).
                    let placeholder_content =
                        Self::replace_images_with_placeholders(&result.content);

                    tool_messages.push(Message::tool_result(ToolResultMessage {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        content: placeholder_content,
                        details: result.details,
                        is_error,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    }));
                }

                // 4. Loop back: provider sees tool results and responds

                // Soft nudge at 40 iterations (placed here, after tool results,
                // to avoid breaking tool_use→tool_result pairing).
                if iterations == 40 {
                    let hint = Message::Custom(CustomMessage {
                        content: "[System note: You've been running tool calls for a while. \
                                  If you're close to done, please wrap up and provide your \
                                  final response. If you still have work to do, continue but \
                                  aim to finish soon.]"
                            .to_string(),
                        custom_type: "system_note".to_string(),
                        display: false,
                        details: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    });
                    self.messages.push(hint);
                }
            }

            let event_msg = Message::assistant(assistant_msg.clone());
            on_event(AgentEvent::TurnEnd {
                session_id: session_id.clone(),
                turn_index: current_turn,
                message: event_msg,
                tool_results: tool_messages,
            });

            turn_index = turn_index.saturating_add(1);
        }

        let final_msg = last_assistant.unwrap_or_else(|| AssistantMessage {
            content: vec![],
            api: self.provider.api().to_string(),
            provider: self.provider.name().to_string(),
            model: self.provider.model_id().to_string(),
            usage: Usage::default(),
            stop_reason: StopReason::Error,
            error_message: Some("Agent completed without response".into()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        });

        on_event(AgentEvent::AgentEnd {
            session_id,
            messages: new_messages,
            error: None,
        });

        Ok(final_msg)
    }
}
