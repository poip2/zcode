//! Context compaction — prevents unbounded token growth in long sessions.
//!
//! Ported and adapted from pi-agent-rust (src/compaction.rs).
//!
//! Core algorithm:
//! 1. Estimate total context tokens from message history
//! 2. If tokens exceed (context_window - reserve), find a cut point
//!    that keeps the most recent `keep_recent_tokens` worth of messages
//! 3. Summarize the discarded messages with the LLM (iteratively updating
//!    any prior summary)
//! 4. Replace old messages with the summary, keeping only the recent ones

use crate::error::{Error, Result};
use crate::model::{
    AssistantMessage, ContentBlock, Message, StopReason, ThinkingLevel,
    Usage, UserContent, UserMessage,
};
use crate::provider::{Context, Provider, StreamOptions};
use futures::StreamExt;
use std::sync::Arc;

// ============================================================================
// Constants
// ============================================================================

/// Approximate characters per token for English text with GPT-family tokenizers.
/// Intentionally conservative (overestimates tokens) to avoid exceeding context windows.
/// Set to 3 to safely account for code/symbol-heavy content which is denser than prose.
const CHARS_PER_TOKEN_ESTIMATE: usize = 3;

/// Estimated tokens for an image content block (~1200 tokens).
const IMAGE_TOKEN_ESTIMATE: usize = 1200;

/// Character-equivalent estimate for an image.
const IMAGE_CHAR_ESTIMATE: usize = IMAGE_TOKEN_ESTIMATE * CHARS_PER_TOKEN_ESTIMATE;

/// Summarization system prompt.  The summarizer must never continue the
/// conversation — it only outputs the structured summary.
const SUMMARIZATION_SYSTEM_PROMPT: &str = "\
You are a context summarization assistant. Your task is to read a conversation \
between a user and an AI coding assistant, then produce a structured summary \
following the exact format specified.\n\
\n\
Do NOT continue the conversation. Do NOT respond to any questions in the \
conversation. ONLY output the structured summary.";

/// Prompt for a fresh (first) summarization.
const SUMMARIZATION_PROMPT: &str = "\
The messages above are a conversation to summarize. Create a structured context \
checkpoint summary that another LLM will use to continue the work.\n\
\n\
Use this EXACT format:\n\
\n\
## Goal\n\
[What is the user trying to accomplish? Can be multiple items if the session \
covers different tasks.]\n\
\n\
## Constraints & Preferences\n\
- [Any constraints, preferences, or requirements mentioned by user]\n\
- [Or \"(none)\" if none were mentioned]\n\
\n\
## Progress\n\
### Done\n\
- [x] [Completed tasks/changes]\n\
\n\
### In Progress\n\
- [ ] [Current work]\n\
\n\
### Blocked\n\
- [Issues preventing progress, if any]\n\
\n\
## Key Decisions\n\
- **[Decision]**: [Brief rationale]\n\
\n\
## Next Steps\n\
1. [Ordered list of what should happen next]\n\
\n\
## Critical Context\n\
- [Any data, examples, or references needed to continue]\n\
- [Or \"(none)\" if not applicable]\n\
\n\
Keep each section concise. Preserve exact file paths, function names, and \
error messages.";

/// Prompt for updating an existing summary with new conversation messages.
const UPDATE_SUMMARIZATION_PROMPT: &str = "\
The messages above are NEW conversation messages to incorporate into the \
existing summary provided in <previous-summary> tags.\n\
\n\
Update the existing structured summary with new information. RULES:\n\
- PRESERVE all existing information from the previous summary\n\
- ADD new progress, decisions, and context from the new messages\n\
- UPDATE the Progress section: move items from \"In Progress\" to \"Done\" \
when completed\n\
- UPDATE \"Next Steps\" based on what was accomplished\n\
- PRESERVE exact file paths, function names, and error messages\n\
- If something is no longer relevant, you may remove it\n\
\n\
Use this EXACT format:\n\
\n\
## Goal\n\
[Preserve existing goals, add new ones if the task expanded]\n\
\n\
## Constraints & Preferences\n\
- [Preserve existing, add new ones discovered]\n\
\n\
## Progress\n\
### Done\n\
- [x] [Include previously done items AND newly completed items]\n\
\n\
### In Progress\n\
- [ ] [Current work - update based on progress]\n\
\n\
### Blocked\n\
- [Current blockers - remove if resolved]\n\
\n\
## Key Decisions\n\
- **[Decision]**: [Brief rationale] (preserve all previous, add new)\n\
\n\
## Next Steps\n\
1. [Update based on current state]\n\
\n\
## Critical Context\n\
- [Preserve important context, add new if needed]\n\
\n\
Keep each section concise. Preserve exact file paths, function names, and \
error messages.";

// ============================================================================
// Compaction Settings
// ============================================================================

/// Configuration for the auto-compaction system.
///
/// All thresholds are percentages of the context window (not absolute tokens),
/// so they scale naturally across models with different window sizes.
///
/// Effective calculation:
///   effective_window = context_window_tokens - min(reserved_output_tokens, 20_000)
///   trigger_at  = effective_window * min(trigger_threshold_pct, 0.90)
///   keep_recent = effective_window * keep_recent_pct
#[derive(Debug, Clone)]
pub struct CompactionSettings {
    /// Whether compaction is enabled at all.
    pub enabled: bool,

    /// Model's context window size in tokens (e.g. 1_000_000 for Gemini, 128_000 for GPT-4o).
    /// Priority: caller passes > model-name lookup > this default.
    pub context_window_tokens: u32,

    /// Compaction trigger threshold, as a fraction of the effective window (0.0–1.0).
    /// Hard-clamped to 0.90 maximum.  Default 0.85.
    pub trigger_threshold_pct: f32,

    /// Fraction of the effective window to keep as recent (unsummarized) messages.
    /// Default 0.15 (15%).
    pub keep_recent_pct: f32,

    /// Tokens reserved for the assistant's output response.
    /// Clamped to max 20_000 regardless of window size.
    pub reserved_output_tokens: u32,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            context_window_tokens: 1_000_000,
            trigger_threshold_pct: 0.85,
            keep_recent_pct: 0.15,
            reserved_output_tokens: 16_384,
        }
    }
}

impl CompactionSettings {
    /// Create settings for a 128K window model.
    pub fn for_128k() -> Self {
        Self {
            context_window_tokens: 128_000,
            ..Default::default()
        }
    }

    /// Create settings for a 32K window model.
    pub fn for_32k() -> Self {
        Self {
            context_window_tokens: 32_000,
            ..Default::default()
        }
    }

    /// Effective context window (minus output reservation, capped at 20K).
    pub fn effective_window(&self) -> u64 {
        let window = u64::from(self.context_window_tokens);
        let reserve = u64::from(self.reserved_output_tokens).min(20_000);
        window.saturating_sub(reserve)
    }

    /// Token count at which compaction triggers.
    pub fn trigger_threshold(&self) -> u64 {
        let effective = self.effective_window();
        let pct = self.trigger_threshold_pct.clamp(0.0, 0.90) as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let threshold = (effective as f64 * pct) as u64;
        threshold.max(1)
    }

    /// Token count to keep as recent messages after compaction.
    pub fn keep_recent_tokens(&self) -> u64 {
        let effective = self.effective_window();
        let pct = self.keep_recent_pct.clamp(0.05, 0.50) as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let keep = (effective as f64 * pct) as u64;
        keep.max(1)
    }

    /// Try to guess the context window from a model name string.
    /// This is a best-effort fallback; callers should explicitly pass the window when possible.
    pub fn guess_from_model(model: &str) -> u32 {
        let lower = model.to_lowercase();

        if lower.contains("gemini-2.5") || lower.contains("gemini-2.0-flash") {
            1_048_576
        } else if lower.contains("gemini") {
            128_000
        } else if lower.contains("claude-opus") || lower.contains("claude-sonnet-4") {
            200_000
        } else if lower.contains("claude") {
            200_000
        } else if lower.contains("gpt-4.1") || lower.contains("gpt-4o") {
            128_000
        } else if lower.contains("gpt-4-turbo") {
            128_000
        } else if lower.contains("gpt-4") {
            128_000
        } else if lower.contains("gpt-3.5") {
            16_384
        } else if lower.contains("deepseek-r1") || lower.contains("deepseek-chat") {
            131_072
        } else if lower.contains("deepseek") {
            131_072
        } else if lower.contains("qwen") {
            131_072
        } else if lower.contains("llama-3") || lower.contains("llama3") {
            131_072
        } else if lower.contains("mistral-large") {
            131_072
        } else if lower.contains("mistral") {
            32_000
        } else {
            128_000
        }
    }
}

// ============================================================================
// Token Estimation
// ============================================================================

/// Count the JSON byte length of a `serde_json::Value` without allocating.
fn json_byte_len(value: &serde_json::Value) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(buf.len());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut c = Counter(0);
    let _ = serde_json::to_writer(&mut c, value);
    c.0
}

/// Heuristically estimate the token count for a single content block.
fn estimate_block_tokens(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text(tc) => tc.text.len(),
        ContentBlock::Thinking(tc) => tc.thinking.len(),
        ContentBlock::Image(_) => IMAGE_CHAR_ESTIMATE,
        ContentBlock::ToolCall(tc) => {
            tc.name.len().saturating_add(json_byte_len(&tc.arguments))
        }
        ContentBlock::RedactedThinking(_) => 0,
    }
}

/// Heuristically estimate the token count for a single message.
///
/// Uses `chars / CHARS_PER_TOKEN_ESTIMATE` as a conservative over-estimate
/// (code-heavy content is denser, so 3 chars/token is safer than 4).
pub fn estimate_message_tokens(msg: &Message) -> u64 {
    let mut chars: usize = 0;

    match msg {
        Message::User(user) => match &user.content {
            UserContent::Text(text) => chars = text.len(),
            UserContent::Blocks(blocks) => {
                for block in blocks {
                    chars = chars.saturating_add(estimate_block_tokens(block));
                }
            }
        },
        Message::Assistant(assistant) => {
            for block in &assistant.content {
                chars = chars.saturating_add(estimate_block_tokens(block));
            }
            // Provider-reported usage is more accurate — prefer it when available
            if assistant.usage.total_tokens > 0 {
                return assistant.usage.total_tokens;
            }
        }
        Message::ToolResult(tr) => {
            for block in &tr.content {
                chars = chars.saturating_add(estimate_block_tokens(block));
            }
        }
        Message::Custom(custom) => {
            chars = custom.content.len();
        }
    }

    u64::try_from(chars.div_ceil(CHARS_PER_TOKEN_ESTIMATE)).unwrap_or(u64::MAX)
}

/// Heuristically estimate the total context tokens across all messages.
///
/// Uses provider-reported usage from the last assistant message when available,
/// falling back to per-message character heuristics.
pub fn estimate_total_tokens(messages: &[Message]) -> u64 {
    // Check if the last message has usage info (provider-reported, most accurate)
    let mut last_usage: Option<&Usage> = None;
    let mut last_usage_idx: Option<usize> = None;

    for (idx, msg) in messages.iter().enumerate().rev() {
        if let Message::Assistant(assistant) = msg {
            if !matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
                if assistant.usage.total_tokens > 0 {
                    last_usage = Some(&assistant.usage);
                    last_usage_idx = Some(idx);
                    break;
                }
            }
        }
    }

    // Use provider-reported total_tokens as the anchor if available.
    // Add heuristic estimates for any messages after that point.
    if let (Some(usage), Some(usage_idx)) = (last_usage, last_usage_idx) {
        let trailing: u64 = messages[usage_idx + 1..]
            .iter()
            .map(estimate_message_tokens)
            .fold(0u64, u64::saturating_add);
        return usage.total_tokens.saturating_add(trailing);
    }

    // Fallback: sum all messages heuristically
    messages
        .iter()
        .map(estimate_message_tokens)
        .fold(0u64, u64::saturating_add)
}

// ============================================================================
// Should Compact?
// ============================================================================

/// Return `true` when the current context is large enough to trigger compaction.
pub fn should_compact(estimated_tokens: u64, settings: &CompactionSettings) -> bool {
    if !settings.enabled {
        return false;
    }
    estimated_tokens >= settings.trigger_threshold()
}

// ============================================================================
// Cut Point Detection
// ============================================================================

/// Find the index in `messages` where we should cut for compaction.
///
/// Returns `None` if there's nothing to cut (everything fits within keep_recent).
/// When a cut is found, `messages[..cut_idx]` will be summarized and
/// `messages[cut_idx..]` will be kept.
///
/// **Cut-rules (must respect tool-use/tool-result pairing):**
/// - Valid cut points: start of a User message, or start of a text-only Assistant
///   message (no tool_use blocks)
/// - Cuts must NOT land inside a tool_use→tool_result chain
/// - If a single turn exceeds keep_recent, cut within the turn at the first
///   text-only Assistant message that satisfies the budget
pub fn find_cut_index(messages: &[Message], keep_recent_tokens: u64) -> Option<usize> {
    if messages.is_empty() {
        return None;
    }

    // Step 1: Walk backwards, accumulating token estimates until we hit the budget.
    let mut accumulated: u64 = 0;
    let mut budget_hit_idx: Option<usize> = None;

    for i in (0..messages.len()).rev() {
        accumulated = accumulated.saturating_add(estimate_message_tokens(&messages[i]));
        if accumulated >= keep_recent_tokens {
            budget_hit_idx = Some(i);
            break;
        }
    }

    // Everything fits — no cut needed
    let Some(budget_idx) = budget_hit_idx else {
        return None;
    };

    // Step 2: From budget_idx forward, find a VALID cut boundary.
    // We scan from budget_idx backwards to find the nearest valid boundary.
    // Then we verify it doesn't split a tool_use→tool_result chain.
    find_nearest_valid_boundary(messages, budget_idx)
}

/// Starting at `budget_idx`, walk backwards to find the nearest valid cut boundary.
/// Returns `None` if no valid boundary exists (shouldn't happen in practice).
fn find_nearest_valid_boundary(messages: &[Message], budget_idx: usize) -> Option<usize> {
    // Walk back to find a valid candidate:
    // - A User message (not from a tool-result group)
    // - An Assistant message with text only (no tool_use blocks)
    let mut candidate = budget_idx;
    loop {
        match &messages[candidate] {
            Message::User(user) => {
                // User messages are valid boundaries.
                // But if this is a "fake" user from tool_result conversion
                // (Anthropic collapses tool results into user messages),
                // we need to go further back.
                // In zcode's model, tool results are ToolResult messages,
                // not user messages, so any User message is a real boundary.
                // Just verify it has actual content (not an empty tool-result user).
                if has_real_user_content(user) {
                    return Some(candidate);
                }
            }
            Message::Assistant(assistant) => {
                // Only valid if NO tool calls (pure text/thinking only)
                if !has_tool_calls(&assistant.content) {
                    return Some(candidate);
                }
                // Has tool calls — skip to before the user message that
                // started this turn
            }
            Message::ToolResult(_) | Message::Custom(_) => {
                // These are never valid boundaries — keep walking back
            }
        }

        if candidate == 0 {
            break;
        }
        candidate -= 1;
    }

    // If we reach here, candidate is 0. Check if it's valid.
    match &messages[0] {
        Message::User(u) if has_real_user_content(u) => Some(0),
        Message::Assistant(a) if !has_tool_calls(&a.content) => Some(0),
        _ => None, // Can't cut at the very beginning if it's not a valid boundary
    }
}

/// Check if a user message has real (non-empty) content.
fn has_real_user_content(user: &UserMessage) -> bool {
    match &user.content {
        UserContent::Text(t) => !t.is_empty(),
        UserContent::Blocks(blocks) => blocks.iter().any(|b| {
            matches!(b, ContentBlock::Text(t) if !t.text.is_empty())
        }),
    }
}

/// Check if any content block is a tool call.
fn has_tool_calls(blocks: &[ContentBlock]) -> bool {
    blocks.iter().any(|b| matches!(b, ContentBlock::ToolCall(_)))
}

// ============================================================================
// Conversation Serialization (for the summarizer LLM)
// ============================================================================

/// Convert content blocks to plain text for summarization.
/// Images are explicitly skipped — do NOT send base64 to the summarizer.
pub fn content_blocks_to_summary_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.clone()),
            ContentBlock::Thinking(t) => Some(format!("[thinking]: {}", t.thinking)),
            ContentBlock::Image(_) => None, // skip images
            ContentBlock::ToolCall(tc) => Some(format!(
                "[tool_call: {}({})]",
                tc.name,
                serde_json::to_string(&tc.arguments).unwrap_or_default()
            )),
            ContentBlock::RedactedThinking(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn push_separator(out: &mut String) {
    if !out.is_empty() {
        out.push_str("\n\n");
    }
}

fn append_user_message(out: &mut String, user: &UserMessage) {
    let text = match &user.content {
        UserContent::Text(t) => t.as_str().to_string(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text(tc) = b {
                    if !tc.text.is_empty() {
                        return Some(tc.text.as_str());
                    }
                }
                None
            })
            .collect::<Vec<_>>()
            .join(""),
    };

    if text.is_empty() {
        return;
    }

    push_separator(out);
    out.push_str("[User]: ");
    out.push_str(&text);
}

fn append_assistant_message(out: &mut String, assistant: &AssistantMessage) {
    let mut has_text = false;
    let mut has_tools = false;

    // Filter out images — never send base64 to the summarizer
    let visible_blocks: Vec<&ContentBlock> = assistant
        .content
        .iter()
        .filter(|b| !matches!(b, ContentBlock::Image(_)))
        .collect();

    // Text + thinking
    push_separator(out);
    out.push_str("[Assistant]: ");
    for block in &visible_blocks {
        match block {
            ContentBlock::Text(tc) => {
                if has_text {
                    out.push('\n');
                }
                out.push_str(&tc.text);
                has_text = true;
            }
            ContentBlock::Thinking(tc) => {
                if has_text {
                    out.push('\n');
                }
                out.push_str("[thinking]: ");
                out.push_str(&tc.thinking);
                has_text = true;
            }
            ContentBlock::ToolCall(_) => {
                has_tools = true;
            }
            _ => {}
        }
    }

    // Tool calls
    if has_tools {
        push_separator(out);
        out.push_str("[Assistant tool calls]: ");
        let mut first = true;
        for block in &visible_blocks {
            if let ContentBlock::ToolCall(tc) = block {
                if !first {
                    out.push_str("; ");
                }
                out.push_str(&tc.name);
                out.push('(');
                if let Some(obj) = tc.arguments.as_object() {
                    let mut first_kv = true;
                    for (k, v) in obj {
                        if !first_kv {
                            out.push_str(", ");
                        }
                        let _ = std::fmt::Write::write_fmt(
                            out,
                            format_args!("{k}={v}"),
                        );
                        first_kv = false;
                    }
                } else if let Ok(s) = serde_json::to_string(&tc.arguments) {
                    out.push_str(&s);
                }
                out.push(')');
                first = false;
            }
        }
    }
}

fn append_tool_result(out: &mut String, result: &crate::model::ToolResultMessage) {
    let mut has_content = false;
    for block in &result.content {
        if let ContentBlock::Text(tc) = block {
            if !tc.text.is_empty() {
                has_content = true;
                break;
            }
        }
    }
    if !has_content {
        return;
    }

    push_separator(out);
    out.push_str("[Tool result");
    out.push_str(&format!(" ({})", result.tool_name));
    out.push_str("]: ");
    for block in &result.content {
        if let ContentBlock::Text(tc) = block {
            out.push_str(&tc.text);
        }
    }
}

fn collect_text_blocks(blocks: &[ContentBlock]) -> String {
    content_blocks_to_summary_text(blocks)
}

/// Serialize a conversation slice into a human-readable format for the summarizer.
fn serialize_conversation(messages: &[Message]) -> String {
    let mut out = String::new();

    for msg in messages {
        match msg {
            Message::User(user) => append_user_message(&mut out, user),
            Message::Assistant(assistant) => append_assistant_message(&mut out, assistant),
            Message::ToolResult(tr) => append_tool_result(&mut out, tr),
            Message::Custom(custom) => {
                if !custom.content.is_empty() {
                    push_separator(&mut out);
                    let _ = std::fmt::Write::write_fmt(
                        &mut out,
                        format_args!("[Custom:{}]: {}", custom.custom_type, custom.content),
                    );
                }
            }
        }
    }

    out
}

// ============================================================================
// Summarization
// ============================================================================

/// Run a simple (non-streaming) completion for the summarization step.
///
/// Returns the collected assistant message.
async fn complete_for_summary(
    provider: Arc<dyn Provider>,
    system_prompt: &str,
    prompt_text: String,
    max_tokens: u32,
) -> Result<AssistantMessage> {
    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text(prompt_text),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];

    // We need to own the messages so we can borrow them for Context.
    // Context takes &'a [Message], but we create a static-like local vec.
    // Safety: we hold `messages` until after `stream` is done.
    let context = Context {
        system_prompt: Some(system_prompt),
        messages: &messages,
        tools: &[],
    };

    let options = StreamOptions {
        max_tokens: Some(max_tokens),
        thinking_level: Some(ThinkingLevel::High),
        ..Default::default()
    };

    let mut stream = provider.stream(&context, &options).await?;
    let mut final_message: Option<AssistantMessage> = None;

    while let Some(event) = stream.next().await {
        match event? {
            crate::model::StreamEvent::Done { message, .. } => {
                final_message = Some(message);
            }
            crate::model::StreamEvent::Error { error, .. } => {
                let msg = error
                    .error_message
                    .unwrap_or_else(|| "Summarization error".to_string());
                return Err(Error::api(msg));
            }
            _ => {}
        }
    }

    let message = final_message.ok_or_else(|| Error::api("Stream ended without Done event"))?;
    if matches!(message.stop_reason, StopReason::Aborted | StopReason::Error) {
        let msg = message
            .error_message
            .unwrap_or_else(|| "Summarization error".to_string());
        return Err(Error::api(msg));
    }
    Ok(message)
}

/// Generate a structured summary from the conversation messages.
///
/// If `previous_summary` is provided, the summarizer will update that summary
/// rather than starting from scratch.
async fn generate_summary(
    messages: &[Message],
    previous_summary: Option<&str>,
    provider: Arc<dyn Provider>,
    reserved_output_tokens: u32,
) -> Result<String> {
    let base_prompt = if previous_summary.is_some() {
        UPDATE_SUMMARIZATION_PROMPT
    } else {
        SUMMARIZATION_PROMPT
    };

    let conversation_text = serialize_conversation(messages);

    let mut prompt_text = format!(
        "<conversation>\n{conversation_text}\n</conversation>\n\n"
    );
    if let Some(previous) = previous_summary {
        let _ = std::fmt::Write::write_fmt(
            &mut prompt_text,
            format_args!(
                "<previous-summary>\n{previous}\n</previous-summary>\n\n"
            ),
        );
    }
    prompt_text.push_str(base_prompt);

    // Calculate max_tokens: use 80% of reserve for fresh summary, 60% for update
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let factor: f64 = if previous_summary.is_some() { 0.6 } else { 0.8 };
    #[allow(clippy::cast_sign_loss)]
    let max_tokens = ((f64::from(reserved_output_tokens) * factor) as u32).max(256);

    let assistant =
        complete_for_summary(provider, SUMMARIZATION_SYSTEM_PROMPT, prompt_text, max_tokens).await?;

    let text = collect_text_blocks(&assistant.content);

    if text.trim().is_empty() {
        return Err(Error::api(
            "Summarization returned empty text; refusing to store empty compaction summary",
        ));
    }

    Ok(text)
}

// ============================================================================
// Public API
// ============================================================================

/// The result of a successful compaction operation.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// The generated structured summary.
    pub summary: String,
    /// Number of messages that were summarized (discarded from context).
    pub messages_summarized: usize,
    /// Number of messages kept after compaction.
    pub messages_kept: usize,
    /// Estimated token count before compaction.
    pub tokens_before: u64,
    /// Estimated token count after compaction (summary + kept messages).
    pub tokens_after: u64,
}

/// Run the full compaction pipeline on the agent's message history.
///
/// 1. Estimates current token usage
/// 2. If under threshold, returns `Ok(None)` — no action needed
/// 3. Finds optimal cut point in message history
/// 4. Calls LLM to summarize discarded messages
/// 5. Returns a `CompactionResult` with the summary
///
/// The caller should replace messages with the summary + kept messages.
pub async fn maybe_compact(
    messages: &[Message],
    previous_summary: Option<&str>,
    provider: Arc<dyn Provider>,
    settings: &CompactionSettings,
) -> Result<Option<CompactionResult>> {
    if messages.is_empty() {
        return Ok(None);
    }

    if !settings.enabled {
        return Ok(None);
    }

    let total_tokens = estimate_total_tokens(messages);
    if !should_compact(total_tokens, settings) {
        return Ok(None);
    }

    let Some(cut_idx) = find_cut_index(messages, settings.keep_recent_tokens()) else {
        return Ok(None);
    };

    if cut_idx == 0 {
        // Nothing to summarize — all messages are recent enough
        return Ok(None);
    }

    let to_summarize = &messages[..cut_idx];
    if to_summarize.is_empty() {
        return Ok(None);
    }

    eprintln!(
        "[zcode] compaction: compacting {} messages ({} tokens), keeping {} messages",
        to_summarize.len(),
        total_tokens,
        messages.len() - cut_idx
    );

    let summary = generate_summary(
        to_summarize,
        previous_summary,
        Arc::clone(&provider),
        settings.reserved_output_tokens,
    )
    .await?;

    let kept = &messages[cut_idx..];
    let tokens_after = estimate_message_tokens(&make_summary_message(&summary))
        .saturating_add(
            kept.iter()
                .map(estimate_message_tokens)
                .fold(0u64, |a, t| a.saturating_add(t)),
        );

    eprintln!(
        "[zcode] compaction: done, summary_len={}, tokens_before={total_tokens}, tokens_after={tokens_after}",
        summary.len()
    );

    Ok(Some(CompactionResult {
        summary,
        messages_summarized: to_summarize.len(),
        messages_kept: kept.len(),
        tokens_before: total_tokens,
        tokens_after,
    }))
}

/// Build a Message that carries the compaction summary as context.
///
/// This is a custom message that providers serialize as a user-like message
/// containing the summary. It replaces all the old summarized messages when
/// building context.
fn make_summary_message(summary: &str) -> Message {
    Message::Custom(crate::model::CustomMessage {
        content: format!(
            "[Context summary from earlier in this conversation]\n\n{summary}",
        ),
        custom_type: "compaction_summary".to_string(),
        display: false,
        details: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
    })
}

/// Apply a compaction result to the agent's message history.
///
/// Replaces the summarized messages (messages up to `cut_idx`) with a single
/// summary message, keeping the recent messages intact.
pub fn apply_compaction(
    messages: &mut Vec<Message>,
    result: &CompactionResult,
) {
    let kept = messages.split_off(messages.len() - result.messages_kept);
    messages.clear();
    messages.push(make_summary_message(&result.summary));
    messages.extend(kept);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TextContent, ToolCall};

    fn make_user_msg(text: &str) -> Message {
        Message::User(UserMessage {
            content: UserContent::Text(text.to_string()),
            timestamp: 0,
        })
    }

    fn make_tool_result(name: &str, text: &str, is_error: bool) -> Message {
        Message::tool_result(crate::model::ToolResultMessage {
            tool_call_id: "call-1".to_string(),
            tool_name: name.to_string(),
            content: vec![ContentBlock::Text(TextContent::new(text))],
            details: None,
            is_error,
            timestamp: 0,
        })
    }

    fn make_assistant_msg(text: &str, usage: Usage) -> Message {
        Message::assistant(AssistantMessage {
            content: vec![ContentBlock::Text(TextContent::new(text))],
            api: "test".to_string(),
            provider: "test".to_string(),
            model: "test".to_string(),
            usage,
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 0,
        })
    }

    #[test]
    fn test_estimate_message_tokens_text() {
        let msg = make_user_msg("hello world"); // 11 chars
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens >= 4); // 11/3 ≈ 4
    }

    #[test]
    fn test_estimate_total_tokens() {
        let messages = vec![
            make_user_msg("hello"),
            make_assistant_msg(
                "hi",
                Usage {
                    total_tokens: 50,
                    input: 30,
                    output: 20,
                    cache_read: 0,
                    cache_write: 0,
                },
            ),
            make_user_msg("how are you?"),
        ];
        let total = estimate_total_tokens(&messages);
        // Should use provider-reported 50 + heuristic for trailing msg
        assert!(total >= 50);
    }

    #[test]
    fn test_should_compact_below_threshold() {
        let settings = CompactionSettings::default();
        assert!(!should_compact(500_000, &settings)); // 500K < 920K threshold
    }

    #[test]
    fn test_should_compact_above_threshold() {
        let settings = CompactionSettings::default();
        assert!(should_compact(950_000, &settings));
    }

    #[test]
    fn test_should_compact_disabled() {
        let settings = CompactionSettings {
            enabled: false,
            ..Default::default()
        };
        assert!(!should_compact(200_000, &settings));
    }

    #[test]
    fn test_find_cut_index_none_when_short() {
        let messages = vec![
            make_user_msg("short"),
            make_assistant_msg("ok", Usage::default()),
        ];
        assert_eq!(find_cut_index(&messages, 1000), None);
    }

    #[test]
    fn test_find_cut_index_finds_boundary() {
        // Create enough messages to exceed keep_recent_tokens
        let mut messages = Vec::new();
        for i in 0..100 {
            messages.push(make_user_msg(&format!("message {i}")));
            messages.push(make_assistant_msg(
                &format!("response {i}"),
                Usage {
                    total_tokens: 100,
                    ..Default::default()
                },
            ));
        }
        // 200 messages, each assistant says 100 total tokens
        // keep_recent = 5000 → should cut well before the end
        let cut = find_cut_index(&messages, 5_000);
        assert!(cut.is_some());
        let idx = cut.unwrap();
        // Cut must be at a valid boundary: User message or text-only Assistant
        match &messages[idx] {
            Message::User(_) => {} // OK
            Message::Assistant(a) => {
                assert!(!a.content.iter().any(|b| matches!(b, ContentBlock::ToolCall(_))));
            }
            _ => panic!("cut at invalid position {idx}: {:?}", messages[idx]),
        }
    }

    #[test]
    fn test_find_cut_index_at_beginning() {
        let messages = vec![
            make_user_msg("test"),
            make_assistant_msg(
                "ok",
                Usage {
                    total_tokens: 10_000,
                    ..Default::default()
                },
            ),
            make_user_msg("hello"),
        ];
        // keep_recent = 100 → the assistant at index 1 (text-only, no tools)
        // is a valid boundary. Index 0 (User "test") gets summarized.
        let cut = find_cut_index(&messages, 100);
        assert_eq!(cut, Some(1));
    }

    #[test]
    fn test_serialize_conversation() {
        let messages = vec![
            make_user_msg("hello"),
            make_assistant_msg("hi there", Usage::default()),
            make_tool_result("read", "file contents here", false),
        ];
        let text = serialize_conversation(&messages);
        assert!(text.contains("[User]: hello"));
        assert!(text.contains("[Assistant]: hi there"));
        assert!(text.contains("[Tool result (read)]"));
        assert!(text.contains("file contents here"));
    }

    #[test]
    fn test_apply_compaction() {
        let mut messages: Vec<Message> = (0..10)
            .map(|i| make_user_msg(&format!("msg {i}")))
            .collect();
        let result = CompactionResult {
            summary: "test summary".to_string(),
            messages_summarized: 7,
            messages_kept: 3,
            tokens_before: 1000,
            tokens_after: 100,
        };
        apply_compaction(&mut messages, &result);
        assert_eq!(messages.len(), 4); // 1 summary + 3 kept
        assert!(matches!(&messages[0], Message::Custom(_)));
    }

    // ── Section 3: tool-use/tool-result boundary tests ──

    fn make_tool_call_assistant(tool_name: &str, args: &str) -> Message {
        Message::assistant(AssistantMessage {
            content: vec![ContentBlock::ToolCall(ToolCall {
                id: "tc-1".to_string(),
                name: tool_name.to_string(),
                arguments: serde_json::json!({"cmd": args}),
                thought_signature: None,
            })],
            api: "test".to_string(),
            provider: "test".to_string(),
            model: "test".to_string(),
            usage: Usage::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            timestamp: 0,
        })
    }

    #[test]
    fn test_cut_never_breaks_tool_chain() {
        // User → Assistant(tool_call) → ToolResult → Assistant(text) → User
        // Cuts should only happen at User or text-only Assistant boundaries
        let messages = vec![
            make_user_msg("do something"),
            make_tool_call_assistant("bash", "ls"),
            make_tool_result("bash", "file1.txt\nfile2.txt", false),
            make_assistant_msg(
                "done",
                Usage {
                    total_tokens: 10_000,
                    ..Default::default()
                },
            ),
            make_user_msg("thanks"),
        ];
        // keep_recent = 50 → should cut at "thanks" (index 4) or "do something" (index 0)
        let cut = find_cut_index(&messages, 50);
        assert!(cut.is_some());
        let idx = cut.unwrap();
        // Must NOT be at the tool_call (index 1) or tool_result (index 2)
        assert!(!matches!(&messages[idx], Message::ToolResult(_)));
        // If it's an Assistant, it must be text-only (index 3 is text-only ✓)
        if let Message::Assistant(a) = &messages[idx] {
            assert!(!a.content.iter().any(|b| matches!(b, ContentBlock::ToolCall(_))));
        }
    }

    #[test]
    fn test_cut_skips_tool_chain_assistant() {
        // User → Assistant(text + tool_call) → ToolResult → Assistant(text)
        // Must NOT cut at the mixed assistant (has tool calls)
        let messages = vec![
            make_user_msg("do something"),
            make_tool_call_assistant("bash", "ls"),
            make_tool_result("bash", "output", false),
            make_assistant_msg(
                "all done",
                Usage {
                    total_tokens: 5_000,
                    ..Default::default()
                },
            ),
        ];
        let cut = find_cut_index(&messages, 10);
        // Should find a valid boundary — either index 0 (user) or index 3 (text-only assistant)
        if let Some(idx) = cut {
            match &messages[idx] {
                Message::User(_) => {} // OK
                Message::Assistant(a) => {
                    assert!(!a.content.iter().any(|b| matches!(b, ContentBlock::ToolCall(_))));
                }
                _ => panic!("cut at invalid position {idx}: {:?}", messages[idx]),
            }
        }
    }

    // ── Section 4&5: image exclusion tests ──

    #[test]
    fn test_content_blocks_to_summary_skips_images() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("hello")),
            ContentBlock::Image(crate::model::ImageContent {
                data: "base64data".to_string(),
                mime_type: "image/png".to_string(),
            }),
            ContentBlock::Text(TextContent::new("world")),
        ];
        let text = content_blocks_to_summary_text(&blocks);
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("base64data")); // Image data excluded
        assert!(!text.contains("image/png"));
    }

    #[test]
    fn test_serialize_conversation_excludes_images() {
        let messages = vec![
            make_user_msg("look at this"),
            Message::assistant(AssistantMessage {
                content: vec![
                    ContentBlock::Text(TextContent::new("I see:")),
                    ContentBlock::Image(crate::model::ImageContent {
                        data: "AAAA".to_string(),
                        mime_type: "image/png".to_string(),
                    }),
                    ContentBlock::Text(TextContent::new("nice")),
                ],
                api: "test".to_string(),
                provider: "test".to_string(),
                model: "test".to_string(),
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            }),
        ];
        let text = serialize_conversation(&messages);
        assert!(text.contains("[User]: look at this"));
        assert!(text.contains("I see:"));
        assert!(text.contains("nice"));
        assert!(!text.contains("AAAA")); // Image data NOT in serialization
    }

    // ── Section 2: percentage-based threshold tests ──

    #[test]
    fn test_percentage_trigger_threshold() {
        let settings = CompactionSettings {
            context_window_tokens: 128_000,
            trigger_threshold_pct: 0.85,
            keep_recent_pct: 0.15,
            reserved_output_tokens: 16_384,
            ..Default::default()
        };
        // effective = 128_000 - 16_384 = 111_616
        // trigger = 111_616 * 0.85 ≈ 94_873
        let threshold = settings.trigger_threshold();
        assert!(threshold > 90_000 && threshold < 100_000);
    }

    #[test]
    fn test_trigger_threshold_clamped_at_90pct() {
        let settings = CompactionSettings {
            context_window_tokens: 100_000,
            trigger_threshold_pct: 0.95, // exceeds 0.90 cap
            keep_recent_pct: 0.15,
            reserved_output_tokens: 16_384,
            ..Default::default()
        };
        // effective = 100_000 - 16_384 = 83_616
        // trigger = 83_616 * 0.90 (clamped) = 75_254
        let threshold = settings.trigger_threshold();
        assert!(threshold < 83_616); // lower than effective window
        assert!(threshold > 70_000);
    }

    // ── model name guessing test ──

    #[test]
    fn test_guess_window_from_model_name() {
        assert_eq!(
            CompactionSettings::guess_from_model("gemini-2.5-pro"),
            1_048_576
        );
        assert_eq!(
            CompactionSettings::guess_from_model("claude-sonnet-4-5-20250929"),
            200_000
        );
        assert_eq!(
            CompactionSettings::guess_from_model("gpt-4o"),
            128_000
        );
        assert_eq!(
            CompactionSettings::guess_from_model("deepseek-chat"),
            131_072
        );
        // Unknown model → conservative default
        assert_eq!(
            CompactionSettings::guess_from_model("some-unknown-model"),
            128_000
        );
    }
}
