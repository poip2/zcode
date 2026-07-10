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
    AssistantMessage, ContentBlock, CustomMessage, Message, StopReason, StreamEvent,
    TextContent, ToolCall, ToolResultMessage, Usage, UserContent, UserMessage,
};
use crate::provider::{Context, Provider, StreamOptions, ToolDef};
use crate::tools::{ToolOutput, ToolRegistry};
use futures::StreamExt;
use serde::Serialize;
use std::sync::Arc;

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
    CompactionStarted {
        reason: String,
        tokens_before: u64,
    },
    /// Context compaction finished.
    CompactionFinished {
        tokens_after: u64,
        summary_len: usize,
    },
    /// Agent was stopped due to stuck tool call loop.
    StuckLoop {
        tool_name: String,
        count: usize,
    },
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
}

impl Agent {
    /// Create a new agent.
    pub fn new(provider: Arc<dyn Provider>, tools: ToolRegistry, config: AgentConfig) -> Self {
        Self {
            provider,
            tools,
            config,
            messages: Vec::new(),
            cached_tool_defs: None,
            compaction_settings: Some(CompactionSettings::default()),
            previous_summary: None,
            recent_tool_calls: Vec::new(),
        }
    }

    /// Get the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Clear message history.
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// Add a message to history.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Replace message history.
    pub fn replace_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
    }

    pub fn provider(&self) -> Arc<dyn Provider> {
        Arc::clone(&self.provider)
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt.as_deref()
    }

    pub fn set_system_prompt(&mut self, prompt: Option<String>) {
        self.config.system_prompt = prompt;
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
    ) -> Result<AssistantMessage> {
        let msg = Message::User(UserMessage {
            content: UserContent::Text(user_input.into()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
        self.run_loop(vec![msg], Arc::new(on_event)).await
    }

    /// Run the agent with a pre-built message list.
    pub async fn run_with_messages(
        &mut self,
        messages: Vec<Message>,
        on_event: impl Fn(AgentEvent) + Send + Sync + 'static,
    ) -> Result<AssistantMessage> {
        self.run_loop(messages, Arc::new(on_event)).await
    }

    // ========================================================================
    // Core Loop
    // ========================================================================

    async fn run_loop(
        &mut self,
        prompts: Vec<Message>,
        on_event: Arc<dyn Fn(AgentEvent) + Send + Sync + 'static>,
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

        let mut has_more_tool_calls = true;

        while has_more_tool_calls {
            let current_turn = turn_index;
            on_event(AgentEvent::TurnStart {
                session_id: session_id.clone(),
                turn_index: current_turn,
            });

            // 1. Check if we need to compact context before sending
            if let Some(ref settings) = self.compaction_settings {
                let estimated = compaction::estimate_total_tokens(&self.messages);
                if compaction::should_compact(estimated, settings) {
                    on_event(AgentEvent::CompactionStarted {
                        reason: format!(
                            "context ({estimated} tokens) exceeds threshold ({})",
                            settings.trigger_threshold()
                        ),
                        tokens_before: estimated,
                    });

                    let provider = Arc::clone(&self.provider);
                    match compaction::maybe_compact(
                        &self.messages,
                        self.previous_summary.as_deref(),
                        provider,
                        settings,
                    )
                    .await
                    {
                        Ok(Some(result)) => {
                            eprintln!(
                                "[zcode] agent::run_loop: compaction applied, {} msgs → {} keyst",
                                result.messages_summarized,
                                result.messages_kept
                            );
                            compaction::apply_compaction(
                                &mut self.messages,
                                &result,
                            );
                            self.previous_summary = Some(result.summary.clone());
                            on_event(AgentEvent::CompactionFinished {
                                tokens_after: result.tokens_after,
                                summary_len: result.summary.len(),
                            });
                        }
                        Ok(None) => {
                            // No compaction needed
                        }
                        Err(e) => {
                            eprintln!(
                                "[zcode] agent::run_loop: compaction FAILED: {e}"
                            );
                            // Continue without compaction — better than losing the session
                        }
                    }
                }
            }

            // 2. Stream assistant response
            self.ensure_tool_defs();
            let context = self.build_context();
            let stream_options = self.config.stream_options.clone();

            let mut stream = match self.provider.stream(&context, &stream_options).await {
                Ok(s) => {
                    eprintln!("[zcode] agent::run_loop: turn #{current_turn} provider.stream() started OK");
                    s
                }
                Err(err) => {
                    eprintln!("[zcode] agent::run_loop: turn #{current_turn} provider.stream() FAILED: {err}");
                    on_event(AgentEvent::AgentEnd {
                        session_id: session_id.clone(),
                        messages: new_messages,
                        error: Some(err.to_string()),
                    });
                    return Err(err);
                }
            };

            let mut assistant_arc: Option<Arc<AssistantMessage>> = None;
            let mut error_occurred = false;

            while let Some(event) = stream.next().await {
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
                    if self.recent_tool_calls.len() >= window {
                        let tail = &self.recent_tool_calls
                            [self.recent_tool_calls.len() - window..];
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

                // Soft nudge at 40 iterations: insert a hint to wrap up
                if iterations == 40 {
                    let hint = Message::Custom(CustomMessage {
                        content: "[System note: You've been running tool calls for a while. \
                                  If you're close to done, please wrap up and provide your \
                                  final response. If you still have work to do, continue but \
                                  aim to finish soon.]".to_string(),
                        custom_type: "system_note".to_string(),
                        display: false,
                        details: None,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    });
                    self.messages.push(hint);
                }

                // 3. Execute tool calls
                for tc in &tool_calls {
                    on_event(AgentEvent::ToolStart {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    });

                    let result = match self.tools.get(&tc.name) {
                        Some(tool) => tool
                            .execute(&tc.id, tc.arguments.clone(), None)
                            .await
                            .unwrap_or_else(|e| ToolOutput {
                                content: vec![ContentBlock::Text(TextContent::new(format!(
                                    "Tool error: {e}"
                                )))],
                                details: None,
                                is_error: true,
                            }),
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

                    // Replace image blocks with text placeholders before storing in history.
                    // Current turn already sent the real image to the provider.
                    let history_content: Vec<ContentBlock> = result.content.iter().map(|block| {
                        match block {
                            ContentBlock::Image(img) => {
                                let size_kb = img.data.len() / 1024;
                                ContentBlock::Text(TextContent::new(format!(
                                    "[已读取图片: {}, {}KB]",
                                    img.mime_type, size_kb
                                )))
                            }
                            other => other.clone(),
                        }
                    }).collect();

                    let tool_result_msg = Message::tool_result(ToolResultMessage {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.name.clone(),
                        content: history_content,
                        details: result.details,
                        is_error,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    });

                    self.messages.push(tool_result_msg.clone());
                    tool_messages.push(tool_result_msg);
                }

                // 4. Loop back: provider sees tool results and responds
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
