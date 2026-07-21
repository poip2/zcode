//! LLM provider abstraction layer.
//!
//! Adapted from pi-agent-rust (src/provider.rs).
//! Defines the `Provider` trait and shared request/response types used by all backends.

use crate::model::{Message, StreamEvent, ThinkingLevel};
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

// ============================================================================
// Provider Trait
// ============================================================================

/// An LLM backend capable of streaming assistant output and tool calls.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g. "anthropic", "openai").
    fn name(&self) -> &str;

    /// API type identifier (e.g. "anthropic-messages", "openai-completions").
    fn api(&self) -> &str;

    /// Model identifier used by this provider (e.g. "claude-sonnet-4-5-20250929").
    fn model_id(&self) -> &str;

    /// Start streaming a completion.
    async fn stream(
        &self,
        context: &Context<'_>,
        options: &StreamOptions,
    ) -> crate::error::Result<Pin<Box<dyn Stream<Item = crate::error::Result<StreamEvent>> + Send>>>;
}

// ============================================================================
// Context
// ============================================================================

/// Inputs to a single completion request.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    /// Provider-specific system prompt content.
    pub system_prompt: Option<&'a str>,
    /// Conversation history (user/assistant/tool results).
    pub messages: &'a [Message],
    /// Tool definitions available to the model for this request.
    pub tools: &'a [ToolDef],
}

// ============================================================================
// Tool Definition
// ============================================================================

/// A tool definition exposed to the model.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ============================================================================
// Stream Options
// ============================================================================

/// Cache retention policy for provider-managed prompt caching.
///
/// Default is `None` (conservative): changing the system-prompt wire format
/// from a plain string to an array of content blocks can break
/// Anthropic-compatible providers (DeepSeek etc.) that reject unknown fields.
/// Enable explicitly for official Anthropic API usage.
///
/// Anthropic ephemeral cache TTL: 5 minutes, reset on every cache read.
/// For an active coding session this means effectively always cached after
/// the first turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CacheRetention {
    #[default]
    None,
    /// Ephemeral cache (5 min TTL, resets on read). ~1.25x write, ~0.10x read.
    Short,
}

/// Options that control streaming completion behavior.
#[derive(Debug, Clone, Default)]
pub struct StreamOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub session_id: Option<String>,
    pub cache_retention: CacheRetention,
    pub headers: HashMap<String, String>,
    pub thinking_level: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
}

/// Custom thinking token budgets per level.
#[derive(Debug, Clone)]
pub struct ThinkingBudgets {
    pub minimal: u32,
    pub low: u32,
    pub medium: u32,
    pub high: u32,
    pub xhigh: u32,
}

// ============================================================================
// HttpError / error types
// ============================================================================

/// A structured error from an API call.
#[derive(Debug, Serialize, Deserialize)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
}
