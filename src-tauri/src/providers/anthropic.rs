//! Anthropic Messages API provider implementation.
//!
//! Adapted from pi-agent-rust (src/providers/anthropic.rs).
//! Implements the Provider trait for the Anthropic Messages API,
//! supporting streaming responses and tool use.

use crate::error::{Error, Result};
use crate::model::{
    AssistantMessage, ContentBlock, Message, RedactedThinkingContent, StopReason, StreamEvent,
    TextContent, ThinkingContent, ThinkingLevel, ToolCall, Usage, UserContent,
};
use crate::provider::{Context, Provider, StreamOptions, ToolDef};
use crate::sse::SseStream;
use async_trait::async_trait;
use futures::stream::{self, Stream};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 8192;

// ============================================================================
// Provider Struct
// ============================================================================

pub struct AnthropicProvider {
    client: reqwest::Client,
    provider: String,
    model: String,
    base_url: String,
    has_api_key: bool,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    pub fn new(
        model: impl Into<String>,
        api_key: Option<impl Into<String>>,
        base_url: Option<impl Into<String>>,
    ) -> Result<Self> {
        let api_key: Option<String> = api_key.map(|k| k.into());
        let has_api_key = api_key.is_some();
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref key) = api_key {
            if key.starts_with("sk-ant-oat") {
                let mut auth =
                    reqwest::header::HeaderValue::from_bytes(format!("Bearer {key}").as_bytes())
                        .map_err(|e| Error::validation(format!("invalid API key: {e}")))?;
                auth.set_sensitive(true);
                headers.insert(reqwest::header::AUTHORIZATION, auth);
            } else {
                let mut api_key_header =
                    reqwest::header::HeaderValue::from_bytes(key.as_bytes())
                        .map_err(|e| Error::validation(format!("invalid API key: {e}")))?;
                api_key_header.set_sensitive(true);
                headers.insert("x-api-key", api_key_header);
            }
        }
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| Error::api(format!("failed to create HTTP client: {e}")))?;

        Ok(Self {
            client,
            provider: "anthropic".to_string(),
            model: model.into(),
            base_url: base_url
                .map(|u| u.into())
                .unwrap_or_else(|| ANTHROPIC_API_URL.to_string()),
            has_api_key,
        })
    }

    fn build_request<'a>(
        &'a self,
        context: &'a Context<'_>,
        options: &StreamOptions,
    ) -> AnthropicRequest<'a> {
        let messages: Vec<AnthropicMessage<'_>> = build_anthropic_messages(context.messages);

        let tools: Option<Vec<AnthropicTool<'_>>> = if context.tools.is_empty() {
            None
        } else {
            Some(
                context
                    .tools
                    .iter()
                    .map(convert_tool_to_anthropic)
                    .collect(),
            )
        };

        // Build thinking config
        let thinking = match options.thinking_level {
            None | Some(ThinkingLevel::Off) => None,
            Some(level) => {
                let budget_tokens = match level {
                    ThinkingLevel::Minimal => 1024,
                    ThinkingLevel::Low => 2048,
                    ThinkingLevel::Medium => 4096,
                    ThinkingLevel::High => 8192,
                    ThinkingLevel::XHigh => 16384,
                    ThinkingLevel::Off => unreachable!(),
                };
                Some(AnthropicThinking {
                    r#type: "enabled",
                    budget_tokens: Some(budget_tokens),
                    display: None,
                })
            }
        };

        let max_tokens = options.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

        AnthropicRequest {
            model: &self.model,
            messages,
            system: context.system_prompt,
            max_tokens,
            temperature: options.temperature,
            tools,
            stream: true,
            thinking,
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.provider
    }

    fn api(&self) -> &'static str {
        "anthropic-messages"
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

        #[cfg(debug_assertions)]
        {
            if let Ok(body_json) = serde_json::to_string_pretty(&request_body) {
                eprintln!("[zcode] anthropic::stream: request body:\n{body_json}");
            }
        }

        let request = self
            .client
            .post(&self.base_url)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("accept", "text/event-stream")
            .json(&request_body);

        // Apply per-request headers
        let request = apply_request_headers(request, options, self.has_api_key);

        let response = request.send().await?;
        let status = response.status();
        eprintln!(
            "[zcode] anthropic::stream: HTTP {status}, url={}",
            self.base_url
        );
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::provider(
                self.name(),
                format!("Anthropic API error (HTTP {status}): {body}"),
            ));
        }

        // Verify content-type is text/event-stream
        let ct = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        if !ct.contains("text/event-stream") {
            return Err(Error::api(format!(
                "Unexpected content-type: {ct}, expected text/event-stream"
            )));
        }

        // Build SSE stream from response bytes
        let byte_stream = response
            .bytes_stream()
            .map(|result| result.map(|b| b.to_vec()).map_err(std::io::Error::other));
        let event_source = SseStream::new(Box::pin(byte_stream));
        eprintln!("[zcode] anthropic::stream: SSE stream built, content-type={ct}");

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
                    match state.event_source.next().await {
                        Some(Ok(msg)) => {
                            if msg.event == "ping" {
                                continue;
                            }
                            match state.process_event(&msg.data) {
                                Ok(Some(event)) => {
                                    if matches!(
                                        &event,
                                        StreamEvent::Done { .. } | StreamEvent::Error { .. }
                                    ) {
                                        state.done = true;
                                    }
                                    return Some((Ok(event), state));
                                }
                                Ok(None) => {} // continue
                                Err(e) => {
                                    state.done = true;
                                    return Some((Err(e), state));
                                }
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
                            return Some((Ok(StreamEvent::Done { reason, message }), state));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}

fn apply_request_headers(
    mut request: reqwest::RequestBuilder,
    options: &StreamOptions,
    has_default_auth: bool,
) -> reqwest::RequestBuilder {
    for (key, value) in &options.headers {
        // Only skip auth headers if the client already has default auth
        if has_default_auth
            && (key.eq_ignore_ascii_case("authorization") || key.eq_ignore_ascii_case("x-api-key"))
        {
            continue;
        }
        request = request.header(key.as_str(), value.as_str());
    }
    request
}

// ============================================================================
// Request Types
// ============================================================================

#[derive(Debug, Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool<'a>>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
}

#[derive(Debug, Serialize)]
struct AnthropicThinking {
    r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    budget_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage<'a> {
    role: &'static str,
    content: Vec<AnthropicContent<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContent<'a> {
    Text {
        text: &'a str,
    },
    Thinking {
        thinking: &'a str,
        signature: &'a str,
    },
    Image {
        source: AnthropicImageSource<'a>,
    },
    ToolUse {
        id: &'a str,
        name: &'a str,
        input: &'a serde_json::Value,
    },
    ToolResult {
        tool_use_id: &'a str,
        content: Vec<AnthropicToolResultContent<'a>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicImageSource<'a> {
    r#type: &'static str,
    media_type: &'a str,
    data: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicToolResultContent<'a> {
    Text { text: &'a str },
}

#[derive(Debug, Serialize)]
struct AnthropicTool<'a> {
    name: &'a str,
    description: &'a str,
    input_schema: &'a serde_json::Value,
}

// ============================================================================
// Response Types (SSE deserialization)
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicMessageStart,
    },
    ContentBlockStart {
        index: u32,
        content_block: AnthropicContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: AnthropicDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: AnthropicMessageDelta,
        #[serde(default)]
        usage: Option<AnthropicDeltaUsage>,
    },
    MessageStop,
    Error {
        error: AnthropicError,
    },
    Ping,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(rename = "input_tokens")]
    input: u64,
    #[serde(default, rename = "cache_read_input_tokens")]
    cache_read: Option<u64>,
    #[serde(default, rename = "cache_creation_input_tokens")]
    cache_write: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDeltaUsage {
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text,
    Thinking,
    RedactedThinking {
        #[serde(default)]
        data: String,
    },
    ToolUse {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum AnthropicDelta {
    TextDelta {
        #[serde(default)]
        text: Option<String>,
    },
    ThinkingDelta {
        #[serde(default)]
        thinking: Option<String>,
    },
    InputJsonDelta {
        #[serde(default)]
        partial_json: Option<String>,
    },
    SignatureDelta {
        #[serde(default)]
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AnthropicStopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    #[serde(default)]
    stop_reason: Option<AnthropicStopReason>,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    message: String,
}

// ============================================================================
// Stream State
// ============================================================================

struct ToolAccum {
    id: String,
    name: String,
    json: String,
}

struct StreamState {
    event_source: SseStream,
    partial: AssistantMessage,
    tool_accums: HashMap<u32, ToolAccum>,
    started_processing: bool,
    done: bool,
}

impl StreamState {
    fn new(event_source: SseStream, model: String, api: String, provider: String) -> Self {
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
            tool_accums: HashMap::new(),
            started_processing: false,
            done: false,
        }
    }

    fn recompute_total_tokens(&mut self) {
        self.partial.usage.total_tokens = self
            .partial
            .usage
            .input
            .saturating_add(self.partial.usage.output)
            .saturating_add(self.partial.usage.cache_read)
            .saturating_add(self.partial.usage.cache_write);
    }

    fn process_event(&mut self, data: &str) -> Result<Option<StreamEvent>> {
        if !self.started_processing {
            self.started_processing = true;
            eprintln!(
                "[zcode] anthropic::stream: first SSE event processing, data_len={}",
                data.len()
            );
        }
        let event: AnthropicStreamEvent = serde_json::from_str(data)
            .map_err(|e| Error::api(format!("JSON parse error: {e}\nData: {data}")))?;

        match event {
            AnthropicStreamEvent::MessageStart { message } => {
                if let Some(usage) = message.usage {
                    self.partial.usage.input = usage.input;
                    self.partial.usage.cache_read = usage.cache_read.unwrap_or(0);
                    self.partial.usage.cache_write = usage.cache_write.unwrap_or(0);
                    self.recompute_total_tokens();
                }
                Ok(Some(StreamEvent::Start {
                    partial: self.partial.clone(),
                }))
            }
            AnthropicStreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                let ci = index as usize;
                match content_block {
                    AnthropicContentBlock::Text => {
                        self.partial
                            .content
                            .push(ContentBlock::Text(TextContent::new("")));
                        Ok(Some(StreamEvent::TextStart { content_index: ci }))
                    }
                    AnthropicContentBlock::Thinking => {
                        self.partial
                            .content
                            .push(ContentBlock::Thinking(ThinkingContent {
                                thinking: String::new(),
                                thinking_signature: None,
                            }));
                        Ok(Some(StreamEvent::ThinkingStart { content_index: ci }))
                    }
                    AnthropicContentBlock::RedactedThinking { data } => {
                        self.partial.content.push(ContentBlock::RedactedThinking(
                            RedactedThinkingContent { data },
                        ));
                        Ok(None)
                    }
                    AnthropicContentBlock::ToolUse { id, name } => {
                        let id = id.unwrap_or_default();
                        let name = name.unwrap_or_default();
                        self.tool_accums.insert(
                            index,
                            ToolAccum {
                                id: id.clone(),
                                name: name.clone(),
                                json: String::new(),
                            },
                        );
                        self.partial.content.push(ContentBlock::ToolCall(ToolCall {
                            id,
                            name,
                            arguments: serde_json::Value::Null,
                            thought_signature: None,
                        }));
                        Ok(Some(StreamEvent::ToolCallStart { content_index: ci }))
                    }
                }
            }
            AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                let ci = index as usize;
                match delta {
                    AnthropicDelta::TextDelta { text } => {
                        if let Some(text) = text {
                            if let Some(ContentBlock::Text(tc)) = self.partial.content.get_mut(ci) {
                                tc.text.push_str(&text);
                            }
                            return Ok(Some(StreamEvent::TextDelta {
                                content_index: ci,
                                delta: text,
                            }));
                        }
                        Ok(None)
                    }
                    AnthropicDelta::ThinkingDelta { thinking } => {
                        if let Some(thinking) = thinking {
                            if let Some(ContentBlock::Thinking(tc)) =
                                self.partial.content.get_mut(ci)
                            {
                                tc.thinking.push_str(&thinking);
                            }
                            return Ok(Some(StreamEvent::ThinkingDelta {
                                content_index: ci,
                                delta: thinking,
                            }));
                        }
                        Ok(None)
                    }
                    AnthropicDelta::InputJsonDelta { partial_json } => {
                        if let Some(json) = partial_json {
                            if let Some(acc) = self.tool_accums.get_mut(&index) {
                                acc.json.push_str(&json);
                            }
                            return Ok(Some(StreamEvent::ToolCallDelta {
                                content_index: ci,
                                delta: json,
                            }));
                        }
                        Ok(None)
                    }
                    AnthropicDelta::SignatureDelta { signature } => {
                        if let Some(sig) = signature {
                            if let Some(ContentBlock::ToolCall(tc)) =
                                self.partial.content.get_mut(ci)
                            {
                                tc.thought_signature = Some(sig);
                            }
                        }
                        Ok(None)
                    }
                }
            }
            AnthropicStreamEvent::ContentBlockStop { index } => {
                let ci = index as usize;
                if let Some(acc) = self.tool_accums.remove(&index) {
                    let arguments: serde_json::Value =
                        serde_json::from_str(&acc.json).unwrap_or(serde_json::Value::Null);
                    if let Some(ContentBlock::ToolCall(tc)) = self.partial.content.get_mut(ci) {
                        tc.arguments = arguments.clone();
                    }
                    return Ok(Some(StreamEvent::ToolCallEnd {
                        content_index: ci,
                        tool_call: ToolCall {
                            id: acc.id,
                            name: acc.name,
                            arguments,
                            thought_signature: None,
                        },
                    }));
                }
                // Text or thinking block end
                if let Some(block) = self.partial.content.get(ci) {
                    match block {
                        ContentBlock::Text(tc) => {
                            return Ok(Some(StreamEvent::TextEnd {
                                content_index: ci,
                                content: tc.text.clone(),
                            }));
                        }
                        ContentBlock::Thinking(tc) => {
                            return Ok(Some(StreamEvent::ThinkingEnd {
                                content_index: ci,
                                content: tc.thinking.clone(),
                            }));
                        }
                        _ => {}
                    }
                }
                Ok(None)
            }
            AnthropicStreamEvent::MessageDelta { delta, usage } => {
                self.partial.stop_reason = match delta.stop_reason {
                    Some(AnthropicStopReason::EndTurn) => StopReason::Stop,
                    Some(AnthropicStopReason::MaxTokens) => StopReason::Length,
                    Some(AnthropicStopReason::ToolUse) => StopReason::ToolUse,
                    Some(AnthropicStopReason::StopSequence) => StopReason::Stop,
                    None => self.partial.stop_reason,
                };
                if let Some(u) = usage {
                    self.partial.usage.output = u.output_tokens;
                    self.recompute_total_tokens();
                }
                Ok(None)
            }
            AnthropicStreamEvent::MessageStop => {
                let reason = self.partial.stop_reason;
                Ok(Some(StreamEvent::Done {
                    reason,
                    message: std::mem::take(&mut self.partial),
                }))
            }
            AnthropicStreamEvent::Error { error } => {
                self.partial.stop_reason = StopReason::Error;
                self.partial.error_message = Some(error.message);
                Ok(Some(StreamEvent::Error {
                    reason: StopReason::Error,
                    error: std::mem::take(&mut self.partial),
                }))
            }
            AnthropicStreamEvent::Ping => Ok(None),
        }
    }
}

// ============================================================================
// Message Conversion
// ============================================================================

/// Build Anthropic messages, collapsing consecutive ToolResult messages into a
/// single user message. Anthropic requires all tool_result blocks to appear in
/// the same user message immediately after an assistant message with tool_use.
fn build_anthropic_messages(messages: &[Message]) -> Vec<AnthropicMessage<'_>> {
    let mut result: Vec<AnthropicMessage<'_>> = Vec::new();

    for msg in messages {
        if let Message::ToolResult(tr) = msg {
            // If the previous message in the output is already a user message,
            // append this tool_result into it (collapse consecutive results).
            let is_tool_result_user = result.last().is_some_and(|m| {
                m.role == "user"
                    && m.content
                        .iter()
                        .any(|c| matches!(c, AnthropicContent::ToolResult { .. }))
            });
            if is_tool_result_user {
                if let Some(AnthropicMessage { content, .. }) = result.last_mut() {
                    for block in &tr.content {
                        content.push(AnthropicContent::ToolResult {
                            tool_use_id: &tr.tool_call_id,
                            content: vec![AnthropicToolResultContent::Text {
                                text: match block {
                                    ContentBlock::Text(t) => t.text.as_str(),
                                    _ => "[non-text content]",
                                },
                            }],
                            is_error: if tr.is_error { Some(true) } else { None },
                        });
                    }
                }
            } else {
                result.push(convert_message_to_anthropic(msg));
            }
        } else {
            result.push(convert_message_to_anthropic(msg));
        }
    }

    result
}

fn convert_message_to_anthropic(message: &Message) -> AnthropicMessage<'_> {
    match message {
        Message::User(user) => AnthropicMessage {
            role: "user",
            content: convert_user_content(&user.content),
        },
        Message::Assistant(assistant) => {
            let mut content = Vec::new();
            for block in &assistant.content {
                if let Some(c) = convert_content_block_to_anthropic(block) {
                    content.push(c);
                }
            }
            AnthropicMessage {
                role: "assistant",
                content,
            }
        }
        Message::ToolResult(result) => {
            let content = result
                .content
                .iter()
                .map(|block| AnthropicContent::ToolResult {
                    tool_use_id: &result.tool_call_id,
                    content: vec![AnthropicToolResultContent::Text {
                        text: match block {
                            ContentBlock::Text(t) => t.text.as_str(),
                            _ => "[non-text content]",
                        },
                    }],
                    is_error: if result.is_error { Some(true) } else { None },
                })
                .collect();
            AnthropicMessage {
                role: "user",
                content,
            }
        }
        Message::Custom(c) => {
            if c.custom_type == "compaction_summary" || c.custom_type == "system_note" {
                AnthropicMessage {
                    role: "user",
                    content: vec![AnthropicContent::Text {
                        text: &c.content,
                    }],
                }
            } else {
                AnthropicMessage {
                    role: "user",
                    content: vec![],
                }
            }
        }
    }
}

fn convert_user_content(content: &UserContent) -> Vec<AnthropicContent<'_>> {
    match content {
        UserContent::Text(text) => {
            vec![AnthropicContent::Text {
                text: text.as_str(),
            }]
        }
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(convert_content_block_to_anthropic)
            .collect(),
    }
}

fn convert_content_block_to_anthropic(block: &ContentBlock) -> Option<AnthropicContent<'_>> {
    match block {
        ContentBlock::Text(tc) => Some(AnthropicContent::Text { text: &tc.text }),
        ContentBlock::Thinking(tc) => Some(AnthropicContent::Thinking {
            thinking: &tc.thinking,
            signature: tc.thinking_signature.as_deref().unwrap_or(""),
        }),
        ContentBlock::Image(img) => Some(AnthropicContent::Image {
            source: AnthropicImageSource {
                r#type: "base64",
                media_type: &img.mime_type,
                data: &img.data,
            },
        }),
        ContentBlock::ToolCall(tc) => Some(AnthropicContent::ToolUse {
            id: &tc.id,
            name: &tc.name,
            input: &tc.arguments,
        }),
        _ => None,
    }
}

fn convert_tool_to_anthropic(tool: &ToolDef) -> AnthropicTool<'_> {
    AnthropicTool {
        name: &tool.name,
        description: &tool.description,
        input_schema: &tool.parameters,
    }
}
