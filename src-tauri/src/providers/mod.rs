//! Provider implementations.
//!
//! Adapted from pi-agent-rust (src/providers/mod.rs).
//! Contains concrete implementations of the Provider trait.

pub mod anthropic;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use openai::OpenAIProvider;

use crate::error::Result;
use std::sync::Arc;

/// Build a provider based on the base_url.
///
/// Detection rule: if `base_url` contains "anthropic" (case-insensitive),
/// use `AnthropicProvider`. Otherwise, fall back to `OpenAIProvider`.
///
/// URL normalization: the provider sends `POST base_url` as-is (no path
/// suffix is auto-appended by the HTTP layer), so we normalize here:
/// - Anthropic: append `/v1/messages` if missing.
/// - OpenAI:   append `/v1/chat/completions` if missing.
///   URLs that already contain the expected suffix are left untouched.
///
/// `provider_name` is an optional label passed through to the OpenAI provider
/// only; the Anthropic provider does not accept a name parameter.
pub fn build_provider(
    provider_name: &str,
    model: &str,
    api_key: &str,
    base_url: &str,
) -> Result<Arc<dyn crate::provider::Provider>> {
    if base_url.to_lowercase().contains("anthropic") {
        let url = normalize_url(base_url, "/v1/messages");
        eprintln!("[zcode] build_provider: ANTHROPIC branch, model={model}, url={url}");
        let p = AnthropicProvider::new(model, Some(api_key), Some(&url))?;
        eprintln!("[zcode] build_provider: AnthropicProvider created OK");
        Ok(Arc::new(p))
    } else {
        let name = if provider_name.is_empty() {
            "openai"
        } else {
            provider_name
        };
        let url = normalize_url(base_url, "/v1/chat/completions");
        eprintln!(
            "[zcode] build_provider: OPENAI branch, provider={name}, model={model}, url={url}"
        );
        let p = OpenAIProvider::new(name, model, Some(api_key), Some(&url))?;
        eprintln!("[zcode] build_provider: OpenAIProvider created OK");
        Ok(Arc::new(p))
    }
}

/// If `base_url` is non-empty and does not already end with `expected_suffix`,
/// append it. Trailing slashes are trimmed before comparison.
fn normalize_url(base_url: &str, expected_suffix: &str) -> String {
    if base_url.is_empty() {
        return base_url.to_string();
    }
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(expected_suffix) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{expected_suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // We only test type routing, not actual network calls.
    // A dummy key avoids keyring side-effects in unit tests.
    const DUMMY_KEY: &str = "sk-dummy-key";

    #[test]
    fn anthropic_deepseek_url() {
        let p = build_provider(
            "",
            "claude-sonnet",
            DUMMY_KEY,
            "https://api.deepseek.com/anthropic",
        )
        .expect("should build");
        assert_eq!(p.name(), "anthropic");
        assert_eq!(p.api(), "anthropic-messages");
    }

    #[test]
    fn anthropic_official_url() {
        let p = build_provider(
            "",
            "claude-sonnet",
            DUMMY_KEY,
            "https://api.anthropic.com/v1/messages",
        )
        .expect("should build");
        assert_eq!(p.name(), "anthropic");
        assert_eq!(p.api(), "anthropic-messages");
    }

    #[test]
    fn anthropic_case_insensitive() {
        let p = build_provider(
            "",
            "claude-sonnet",
            DUMMY_KEY,
            "https://api.deepseek.com/AnThRoPiC",
        )
        .expect("should build");
        assert_eq!(p.name(), "anthropic");
    }

    #[test]
    fn openai_default() {
        let p = build_provider("", "gpt-4o", DUMMY_KEY, "https://api.openai.com/v1")
            .expect("should build");
        assert_eq!(p.api(), "openai-completions");
    }

    #[test]
    fn empty_base_url_fallsback_openai() {
        let p = build_provider("custom", "gpt-4o", DUMMY_KEY, "").expect("should build");
        assert_eq!(p.name(), "custom");
        assert_eq!(p.api(), "openai-completions");
    }

    #[test]
    fn generic_url_uses_openai() {
        let p = build_provider(
            "deepseek",
            "deepseek-chat",
            DUMMY_KEY,
            "https://api.deepseek.com/v1/chat/completions",
        )
        .expect("should build");
        assert_eq!(p.name(), "deepseek");
        assert_eq!(p.api(), "openai-completions");
    }

    #[test]
    fn openrouter_uses_openai() {
        let p = build_provider(
            "openrouter",
            "openai/gpt-4o",
            DUMMY_KEY,
            "https://openrouter.ai/api/v1/chat/completions",
        )
        .expect("should build");
        assert_eq!(p.name(), "openrouter");
        assert_eq!(p.api(), "openai-completions");
    }

    // ── URL normalization ──

    #[test]
    fn normalize_anthropic_appends_messages() {
        let p = build_provider(
            "",
            "claude-sonnet",
            DUMMY_KEY,
            "https://api.deepseek.com/anthropic",
        )
        .expect("should build");
        // model_id is the first constructor arg, which is the model name
        assert_eq!(p.api(), "anthropic-messages");
    }

    #[test]
    fn normalize_anthropic_already_complete_untouched() {
        let p = build_provider(
            "",
            "claude-sonnet",
            DUMMY_KEY,
            "https://api.deepseek.com/anthropic/v1/messages",
        )
        .expect("should build");
        assert_eq!(p.api(), "anthropic-messages");
    }

    #[test]
    fn normalize_openai_appends_completions() {
        let p = build_provider("openai", "gpt-4o", DUMMY_KEY, "https://api.openai.com/v1")
            .expect("should build");
        assert_eq!(p.api(), "openai-completions");
    }

    #[test]
    fn normalize_openai_already_complete_untouched() {
        let p = build_provider(
            "openai",
            "gpt-4o",
            DUMMY_KEY,
            "https://api.openai.com/v1/chat/completions",
        )
        .expect("should build");
        assert_eq!(p.api(), "openai-completions");
    }

    #[test]
    fn normalize_trailing_slash_trimmed() {
        let p = build_provider(
            "",
            "claude-sonnet",
            DUMMY_KEY,
            "https://api.deepseek.com/anthropic/",
        )
        .expect("should build");
        assert_eq!(p.api(), "anthropic-messages");
    }

    #[test]
    fn normalize_empty_url_returns_default() {
        let p = build_provider("openai", "gpt-4o", DUMMY_KEY, "").expect("should build");
        assert_eq!(p.api(), "openai-completions");
        // Empty URL → provider uses its built-in default
    }

    // ── Unit test for the helper itself ──

    #[test]
    fn test_normalize_url_appends() {
        assert_eq!(
            normalize_url("https://api.x.com/anthropic", "/v1/messages"),
            "https://api.x.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn test_normalize_url_trailing_slash() {
        assert_eq!(
            normalize_url("https://api.x.com/anthropic/", "/v1/messages"),
            "https://api.x.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn test_normalize_url_already_complete() {
        assert_eq!(
            normalize_url("https://api.x.com/anthropic/v1/messages", "/v1/messages"),
            "https://api.x.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn test_normalize_url_empty() {
        assert_eq!(normalize_url("", "/v1/messages"), "");
    }
}
