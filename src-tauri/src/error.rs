//! Error types for the zcode provider/tools/agent pipeline.
//!
//! Adapted from pi-agent-rust (src/error.rs).

use serde::Serialize;

/// Primary error type for the application.
#[derive(Debug)]
pub enum Error {
    /// Provider/API-level errors (HTTP errors, auth failures, rate limits).
    Provider { provider: String, message: String },
    /// Tool execution errors.
    Tool { tool: String, message: String },
    /// Input validation errors.
    Validation(String),
    /// JSON serialization/deserialization errors.
    Api(String),
    /// SSE stream parsing errors.
    Sse(String),
    /// I/O errors.
    Io(std::io::Error),
    /// Generic/other errors.
    Other(anyhow::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider { provider, message } => {
                write!(f, "[{provider}] {message}")
            }
            Self::Tool { tool, message } => write!(f, "[{tool}] {message}"),
            Self::Validation(msg) => write!(f, "Validation error: {msg}"),
            Self::Api(msg) => write!(f, "API error: {msg}"),
            Self::Sse(msg) => write!(f, "SSE error: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Api(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Api(err.to_string())
    }
}

impl Error {
    pub fn provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Provider {
            provider: provider.into(),
            message: message.into(),
        }
    }

    pub fn tool(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Tool {
            tool: tool.into(),
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn api(message: impl Into<String>) -> Self {
        Self::Api(message.into())
    }

    pub fn sse(message: impl Into<String>) -> Self {
        Self::Sse(message.into())
    }
}

/// Result type alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Serialize error to JSON for Tauri invoke responses.
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub kind: String,
}

impl From<&Error> for ErrorResponse {
    fn from(err: &Error) -> Self {
        let kind = match err {
            Error::Provider { .. } => "provider",
            Error::Tool { .. } => "tool",
            Error::Validation(_) => "validation",
            Error::Api(_) => "api",
            Error::Sse(_) => "sse",
            Error::Io(_) => "io",
            Error::Other(_) => "other",
        };
        Self {
            error: err.to_string(),
            kind: kind.to_string(),
        }
    }
}
