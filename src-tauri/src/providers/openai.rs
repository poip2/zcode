//! OpenAI Chat Completions API provider implementation.
//!
//! Adapted from pi-agent-rust (src/providers/openai.rs).
//! Implements the Provider trait for the OpenAI Chat Completions API,
//! supporting streaming responses and tool use. Compatible with:
//! - OpenAI direct API (api.openai.com)
//! - Any OpenAI-compatible API (Groq, DeepSeek, OpenRouter, Together, etc.)

use crate::error::{Error, Result};
use crate::model::{
    AssistantMessage, ContentBlock, Message, StopReason, StreamEvent, TextContent,
    ThinkingContent, ThinkingLevel, ToolCall, Usage, UserContent,
};
use crate::provider::{Context, Provider, StreamOptions, ToolDef};
use crate::sse::SseStream;
use async_trait::async_trait;
use futures::stream::{self, Stream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::pin::Pin;

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MAX_TOKENS: u32 = 4096;

// ============================================================================
// Provider Struct
// ============================================================================

pub struct OpenAIProvider {
    client: reqwest::Client,
    provider: String,
    model: String,
    base_url: String,
    has_api_key: bool,
}

impl OpenAIProvider {
    /// Create a new OpenAI-compatible provider.
    ///
    /// # Arguments
    /// * `provider` - Provider name (e.g. "openai", "deepseek", "groq")
    /// * `model` - Model identifier
    /// * `api_key` - Optional API key (falls back to env var)
    /// * `base_url` - Optional base URL (falls back to OpenAI default)
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<impl Into<String>>,
        base_url: Option<impl Into<String>>,
    ) -> Self {
        let api_key: Option<String> = api_key.map(|k| k.into());
        let has_api_key = api_key.is_some();
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref key) = api_key {
            let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))
                .expect("invalid api key");
            auth.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, auth);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("failed to create HTTP client");

        Self {
            client,
            provider: provider.into(),
            model: model.into(),
            base_url: base_url
                .map(|u| u.into())
                .unwrap_or_else(|| OPENAI_API_URL.to_string()),
            has_api_key,
        }
    }

    /// Create an OpenAI provider (alias for convenience).
    pub fn openai(model: impl Into<String>, api_key: Option<impl Into<String>>) -> Self {
        Self::new("openai", model, api_key, None::<String>)
    }

    /// Create a DeepSeek provider (uses deepseek API).
    pub fn deepseek(model: impl Into<String>, api_key: Option<impl Into<String>>) -> Self {
        Self::new(
            "deepseek",
            model,
            api_key,
            Some("https://api.deepseek.com/chat/completions"),
        )
    }

    /// Create a Groq provider.
    pub fn groq(model: impl Into<String>, api_key: Option<impl Into<String>>) -> Self {
        Self::new(
            "groq",
            model,
            api_key,
            Some("https://api.groq.com/openai/v1/chat/completions"),
        )
    }

    /// Create an OpenRouter provider.
    pub fn openrouter(model: impl Into<String>, api_key: Option<impl Into<String>>) -> Self {
        Self::new(
            "openrouter",
            model,
            api_key,
            Some("https://openrouter.ai/api/v1/chat/completions"),
        )
    }

    fn build_request<'a>(
        &'a self,
        context: &'a Context<'_>,
        options: &StreamOptions,
    ) -> OpenAIRequest<'a> {
        let mut messages: Vec<OpenAIMessage<'_>> =
            Vec::with_capacity(context.messages.len() + 1);

        // Add system prompt as first message
        if let Some(system) = context.system_prompt {
            messages.push(OpenAIMessage {
                role: "system",
                content: Some(OpenAIContent::Text(Cow::Borrowed(system))),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        // Convert conversation messages
        for message in context.messages.iter() {
            messages.extend(convert_message_to_openai(message));
        }

        let tools: Option<Vec<OpenAITool<'_>>> = if context.tools.is_empty() {
            None
        } else {
            Some(context.tools.iter().map(convert_tool_to_openai).collect())
        };

        // Thinking/reasoning for DeepSeek
        let (thinking, reasoning_effort) = if self.provider.eq_ignore_ascii_case("deepseek") {
            match options.thinking_level.unwrap_or_default() {
                ThinkingLevel::Off => (
                    Some(OpenAIThinking { kind: "disabled" }),
                    None,
                ),
                ThinkingLevel::XHigh => (
                    Some(OpenAIThinking { kind: "enabled" }),
                    Some("max"),
                ),
                _ => (
                    Some(OpenAIThinking { kind: "enabled" }),
                    None,
                ),
            }
        } else {
            (None, None)
        };

        OpenAIRequest {
            model: &self.model,
            messages,
            max_tokens: Some(options.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)),
            max_completion_tokens: None,
            temperature: options.temperature,
            tools,
            stream: true,
            stream_options: Some(OpenAIStreamOptions {
                include_usage: true,
            }),
            thinking,
            reasoning_effort,
        }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        &self.provider
    }

    fn api(&self) -> &'static str {
        "openai-completions"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    async fn stream(
        &self,
        context: &Context<'_>,
        options: &StreamOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let request_body = self.build_request(context, options);

        let mut request = self
            .client
            .post(&self.base_url)
            .header("accept", "text/event-stream")
            .json(&request_body);

        // Apply per-request headers (only skip auth if client has default auth)
        for (key, value) in &options.headers {
            if self.has_api_key && key.eq_ignore_ascii_case("authorization") {
                continue;
            }
            request = request.header(key.as_str(), value.as_str());
        }

        let response: reqwest::Response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::provider(
                self.name(),
                format!("OpenAI API error (HTTP {status}): {body}"),
            ));
        }

        // Build SSE stream
        let byte_stream = response
            .bytes_stream()
            .map(|result| {
                result
                    .map(|b| b.to_vec())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            });
        let event_source = SseStream::new(Box::pin(byte_stream));

        let model = self.model.clone();
        let api = self.api().to_string();
        let provider = self.name().to_string();

        let stream = stream::unfold(
            StreamState::new(event_source, model, api, provider),
            |mut state| async move {
                if state.done {
                    return None;
                }
                loop {
                    // Drain pending events first
                    if let Some(event) = state.pending_events.pop_front() {
                        return Some((Ok(event), state));
                    }

                    match state.event_source.next().await {
                        Some(Ok(msg)) => {
                            // OpenAI sends "[DONE]" as final message
                            if msg.data == "[DONE]" {
                                state.done = true;
                                let reason = state.partial.stop_reason;
                                let message = std::mem::take(&mut state.partial);
                                return Some((
                                    Ok(StreamEvent::Done { reason, message }),
                                    state,
                                ));
                            }

                            if let Err(e) = state.process_event(&msg.data) {
                                state.done = true;
                                return Some((Err(e), state));
                            }
                        }
                        Some(Err(e)) => {
                            state.done = true;
                            return Some((Err(Error::sse(e.to_string())), state));
                        }
                        None => {
                            state.done = true;
                            let reason = state.partial.stop_reason;
                            let message = std::mem::take(&mut state.partial);
                            return Some((
                                Ok(StreamEvent::Done { reason, message }),
                                state,
                            ));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}

// ============================================================================
// Request Types
// ============================================================================

#[derive(Debug, Serialize)]
struct OpenAIRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool<'a>>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<OpenAIStreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<OpenAIThinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct OpenAIStreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct OpenAIThinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAIContent<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCallRef<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAIContent<'a> {
    Text(Cow<'a, str>),
    Parts(Vec<OpenAIContentPart<'a>>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAIContentPart<'a> {
    Text { text: Cow<'a, str> },
    ImageUrl { image_url: OpenAIImageUrl },
}

#[derive(Debug, Serialize)]
struct OpenAIImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct OpenAIToolCallRef<'a> {
    id: Cow<'a, str>,
    r#type: &'static str,
    function: OpenAIFunctionRef<'a>,
}

#[derive(Debug, Serialize)]
struct OpenAIFunctionRef<'a> {
    name: Cow<'a, str>,
    arguments: Cow<'a, str>,
}

#[derive(Debug, Serialize)]
struct OpenAITool<'a> {
    r#type: &'static str,
    function: OpenAIFunction<'a>,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction<'a> {
    name: Cow<'a, str>,
    description: Cow<'a, str>,
    parameters: Cow<'a, serde_json::Value>,
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
    #[serde(default)]
    error: Option<OpenAIChunkError>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    delta: OpenAIDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAIFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: Option<u64>,
    total_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: Option<OpenAIPromptTokensDetails>,
    #[serde(default)]
    prompt_cache_miss_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenAIPromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChunkError {
    #[serde(default)]
    message: Option<String>,
}

// ============================================================================
// Stream State
// ============================================================================

struct ToolCallState {
    id: String,
    name: String,
    arguments: String,
}

struct StreamState {
    event_source: SseStream,
    partial: AssistantMessage,
    tool_calls: Vec<ToolCallState>,
    pending_events: VecDeque<StreamEvent>,
    started: bool,
    done: bool,
}

impl StreamState {
    fn new(
        event_source: SseStream,
        model: String,
        api: String,
        provider: String,
    ) -> Self {
        Self {
            event_source,
            partial: AssistantMessage {
                content: Vec::new(),
                api,
                provider,
                model,
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
            tool_calls: Vec::new(),
            pending_events: VecDeque::new(),
            started: false,
            done: false,
        }
    }

    fn ensure_started(&mut self) {
        if !self.started {
            self.started = true;
            self.pending_events.push_back(StreamEvent::Start {
                partial: self.partial.clone(),
            });
        }
    }

    fn process_event(&mut self, data: &str) -> Result<()> {
        let chunk: OpenAIStreamChunk = serde_json::from_str(data)
            .map_err(|e| Error::api(format!("JSON parse error: {e}\nData: {data}")))?;

        // Handle usage in final chunk
        if let Some(usage) = chunk.usage {
            let cached = usage
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0);
            self.partial.usage.cache_read = cached;
            self.partial.usage.input = usage
                .prompt_cache_miss_tokens
                .unwrap_or_else(|| usage.prompt_tokens.saturating_sub(cached));
            self.partial.usage.output = usage.completion_tokens.unwrap_or(0);
            self.partial.usage.total_tokens = usage.total_tokens;
        }

        if let Some(error) = chunk.error {
            self.partial.stop_reason = StopReason::Error;
            if let Some(msg) = error.message {
                let msg = msg.trim().to_string();
                if !msg.is_empty() {
                    self.partial.error_message = Some(msg);
                }
            }
        }

        // Process first choice
        if let Some(choice) = chunk.choices.into_iter().next() {
            self.process_choice(choice);
        }

        Ok(())
    }

    fn process_choice(&mut self, choice: OpenAIChoice) {
        // Emit Start event on first meaningful delta
        if !self.started
            && choice.finish_reason.is_none()
            && choice.delta.content.is_none()
            && choice.delta.reasoning_content.is_none()
            && choice.delta.tool_calls.is_none()
        {
            self.ensure_started();
            return;
        }

        self.ensure_started();

        let has_content = choice.delta.content.is_some();
        let has_reasoning = choice.delta.reasoning_content.is_some();
        let has_tool_calls = choice.delta.tool_calls.is_some();
        let has_finish = choice.finish_reason.is_some();

        // No delta in this chunk — just emit start
        if !has_content && !has_reasoning && !has_tool_calls && !has_finish {
            return;
        }

        // Content delta
        if let Some(text) = choice.delta.content {
            let ci = self.ensure_text_block();
            if let Some(ContentBlock::Text(tc)) = self.partial.content.get_mut(ci) {
                tc.text.push_str(&text);
            }
            self.pending_events.push_back(StreamEvent::TextDelta {
                content_index: ci,
                delta: text,
            });
        }

        // Reasoning (thinking) delta
        if let Some(reasoning) = choice.delta.reasoning_content {
            let ci = self.ensure_thinking_block();
            if let Some(ContentBlock::Thinking(tc)) = self.partial.content.get_mut(ci) {
                tc.thinking.push_str(&reasoning);
            }
            self.pending_events.push_back(StreamEvent::ThinkingDelta {
                content_index: ci,
                delta: reasoning,
            });
        }

        // Tool call deltas
        if let Some(tool_call_deltas) = choice.delta.tool_calls {
            for tc_delta in tool_call_deltas {
                let idx = tc_delta.index as usize;

                // Ensure we have state for this tool call
                while self.tool_calls.len() <= idx {
                    self.tool_calls.push(ToolCallState {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                    // Add a placeholder ToolCall to the partial
                    let ci = self.partial.content.len();
                    self.partial.content.push(ContentBlock::ToolCall(ToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments: serde_json::Value::Null,
                        thought_signature: None,
                    }));
                    self.pending_events.push_back(StreamEvent::ToolCallStart {
                        content_index: ci,
                    });
                }

                let tc = &mut self.tool_calls[idx];
                if let Some(id) = tc_delta.id {
                    tc.id = id;
                }
                if let Some(func) = tc_delta.function {
                    if let Some(name) = func.name {
                        tc.name = name;
                    }
                    if let Some(args) = func.arguments {
                        tc.arguments.push_str(&args);
                        let ci = self.find_tool_call_content_index(idx);
                        self.pending_events.push_back(StreamEvent::ToolCallDelta {
                            content_index: ci,
                            delta: args,
                        });
                    }
                }
            }
        }

        // Finish reason
        if let Some(finish_reason) = choice.finish_reason {
            self.partial.stop_reason = match finish_reason.as_str() {
                "stop" => StopReason::Stop,
                "length" => StopReason::Length,
                "tool_calls" => StopReason::ToolUse,
                "content_filter" => StopReason::Error,
                _ => StopReason::Stop,
            };

            // Finalize tool calls
            if finish_reason == "tool_calls" || finish_reason == "stop" {
                for (idx, tc) in self.tool_calls.iter().enumerate() {
                    let ci = self.find_tool_call_content_index(idx);
                    let arguments: serde_json::Value =
                        serde_json::from_str(&tc.arguments).unwrap_or(serde_json::Value::Null);

                    // Update the partial
                    if let Some(ContentBlock::ToolCall(block)) =
                        self.partial.content.get_mut(ci)
                    {
                        block.id = tc.id.clone();
                        block.name = tc.name.clone();
                        block.arguments = arguments.clone();
                    }

                    self.pending_events.push_back(StreamEvent::ToolCallEnd {
                        content_index: ci,
                        tool_call: ToolCall {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            arguments,
                            thought_signature: None,
                        },
                    });
                }
            } else {
                // Emit text/thinking end events
                for (ci, block) in self.partial.content.iter().enumerate() {
                    match block {
                        ContentBlock::Text(tc) => {
                            self.pending_events.push_back(StreamEvent::TextEnd {
                                content_index: ci,
                                content: tc.text.clone(),
                            });
                        }
                        ContentBlock::Thinking(tc) => {
                            self.pending_events.push_back(StreamEvent::ThinkingEnd {
                                content_index: ci,
                                content: tc.thinking.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn ensure_text_block(&mut self) -> usize {
        // Check if last block is already a text block
        if let Some(ContentBlock::Text(_)) = self.partial.content.last() {
            self.partial.content.len() - 1
        } else {
            let ci = self.partial.content.len();
            self.partial
                .content
                .push(ContentBlock::Text(TextContent::new("")));
            self.pending_events.push_back(StreamEvent::TextStart {
                content_index: ci,
            });
            ci
        }
    }

    fn ensure_thinking_block(&mut self) -> usize {
        // Check if last thinking block
        if let Some(ContentBlock::Thinking(_)) = self.partial.content.last() {
            self.partial.content.len() - 1
        } else {
            let ci = self.partial.content.len();
            self.partial.content.push(ContentBlock::Thinking(ThinkingContent {
                thinking: String::new(),
                thinking_signature: None,
            }));
            self.pending_events.push_back(StreamEvent::ThinkingStart {
                content_index: ci,
            });
            ci
        }
    }

    fn find_tool_call_content_index(&self, tool_call_idx: usize) -> usize {
        let mut tool_count = 0;
        for (ci, block) in self.partial.content.iter().enumerate() {
            if matches!(block, ContentBlock::ToolCall(_)) {
                if tool_count == tool_call_idx {
                    return ci;
                }
                tool_count += 1;
            }
        }
        // Fallback: return the last index
        self.partial.content.len().saturating_sub(1)
    }
}

// ============================================================================
// Message Conversion
// ============================================================================

fn convert_message_to_openai<'a>(message: &'a Message) -> Vec<OpenAIMessage<'a>> {
    match message {
        Message::User(user) => vec![OpenAIMessage {
            role: "user",
            content: Some(convert_user_content(&user.content)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        Message::Assistant(assistant) => {
            let tool_calls: Vec<OpenAIToolCallRef<'a>> = assistant
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolCall(tc) => Some(OpenAIToolCallRef {
                        id: Cow::Borrowed(&tc.id),
                        r#type: "function",
                        function: OpenAIFunctionRef {
                            name: Cow::Borrowed(&tc.name),
                            arguments: Cow::Owned(tc.arguments.to_string()),
                        },
                    }),
                    _ => None,
                })
                .collect();

            let has_text = assistant
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::Text(_)) || matches!(block, ContentBlock::Thinking(_)));

            if has_text || tool_calls.is_empty() {
                let content = if tool_calls.is_empty() {
                    let text: String = assistant
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text(tc) => Some(tc.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    Some(OpenAIContent::Text(Cow::Owned(text)))
                } else {
                    None
                };

                vec![OpenAIMessage {
                    role: "assistant",
                    content,
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                    name: None,
                }]
            } else if !tool_calls.is_empty() {
                vec![OpenAIMessage {
                    role: "assistant",
                    content: None,
                    tool_calls: Some(tool_calls),
                    tool_call_id: None,
                    name: None,
                }]
            } else {
                vec![]
            }
        }
        Message::ToolResult(result) => {
            let content_text: String = result
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text(tc) => Some(tc.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");

            vec![OpenAIMessage {
                role: "tool",
                content: Some(OpenAIContent::Text(Cow::Owned(content_text))),
                tool_calls: None,
                tool_call_id: Some(Cow::Owned(result.tool_call_id.clone())),
                name: None,
            }]
        }
        Message::Custom(_) => vec![],
    }
}

fn convert_user_content<'a>(content: &'a UserContent) -> OpenAIContent<'a> {
    match content {
        UserContent::Text(text) => OpenAIContent::Text(Cow::Borrowed(text.as_str())),
        UserContent::Blocks(blocks) => {
            let parts: Vec<OpenAIContentPart<'a>> = blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text(tc) => {
                        Some(OpenAIContentPart::Text { text: Cow::Borrowed(&tc.text) })
                    }
                    ContentBlock::Image(img) => {
                        Some(OpenAIContentPart::ImageUrl {
                            image_url: OpenAIImageUrl {
                                url: format!("data:{};base64,{}", img.mime_type, img.data),
                            },
                        })
                    }
                    _ => None,
                })
                .collect();
            OpenAIContent::Parts(parts)
        }
    }
}

fn convert_tool_to_openai(tool: &ToolDef) -> OpenAITool<'_> {
    OpenAITool {
        r#type: "function",
        function: OpenAIFunction {
            name: Cow::Borrowed(&tool.name),
            description: Cow::Borrowed(&tool.description),
            parameters: Cow::Borrowed(&tool.parameters),
        },
    }
}
