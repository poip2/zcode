//! Tool trait, registry, and shared types.
//!
//! Adapted from pi-agent-rust (src/tools.rs).

pub mod bash;
pub mod edit;
pub mod find;
pub mod grep;
pub mod ls;
pub mod read;
pub mod write;

use crate::error::{Error, Result};
use crate::model::ContentBlock;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ============================================================================
// Tool Trait
// ============================================================================

/// Coarse side-effect declaration for tool scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolEffects {
    bits: u8,
}

impl ToolEffects {
    const READ: u8 = 1 << 0;
    const WRITE: u8 = 1 << 1;
    const APPEND: u8 = 1 << 2;
    const NETWORK: u8 = 1 << 3;
    const PROCESS: u8 = 1 << 4;

    pub const fn read() -> Self {
        Self { bits: Self::READ }
    }
    pub const fn write() -> Self {
        Self { bits: Self::WRITE }
    }
    pub const fn append() -> Self {
        Self { bits: Self::APPEND }
    }
    pub const fn network() -> Self {
        Self { bits: Self::NETWORK }
    }
    pub const fn process() -> Self {
        Self { bits: Self::PROCESS }
    }
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }
    pub const fn reads(self) -> bool {
        self.bits & Self::READ != 0
    }
    pub const fn writes(self) -> bool {
        self.bits & Self::WRITE != 0
    }
}

/// A tool that can be executed by the agent.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name.
    fn name(&self) -> &str;

    /// Get the tool label (display name).
    fn label(&self) -> &str;

    /// Get the tool description.
    fn description(&self) -> &str;

    /// Get the tool parameters as JSON Schema.
    fn parameters(&self) -> serde_json::Value;

    /// Execute the tool.
    async fn execute(
        &self,
        tool_call_id: &str,
        input: serde_json::Value,
        on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    ) -> Result<ToolOutput>;

    /// Declare the coarse side effects used by the agent scheduler.
    fn effects(&self) -> ToolEffects {
        ToolEffects::write()
    }
}

/// Tool execution output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    pub content: Vec<ContentBlock>,
    pub details: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_error: bool,
}

const fn is_false(value: &bool) -> bool {
    !*value
}

/// Incremental update during tool execution.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdate {
    pub content: Vec<ContentBlock>,
    pub details: Option<serde_json::Value>,
}

// ============================================================================
// Tool Registry
// ============================================================================

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new registry with the specified tools enabled.
    pub fn new(enabled: &[&str], cwd: &Path) -> Self {
        use bash::BashTool;
        use edit::EditTool;
        use find::FindTool;
        use grep::GrepTool;
        use ls::LsTool;
        use read::ReadTool;
        use write::WriteTool;

        let mut tools: Vec<Box<dyn Tool>> = Vec::new();
        for name in enabled {
            match *name {
                "read" => tools.push(Box::new(ReadTool::new(cwd))),
                "bash" => tools.push(Box::new(BashTool::new(cwd))),
                "edit" => tools.push(Box::new(EditTool::new(cwd))),
                "write" => tools.push(Box::new(WriteTool::new(cwd))),
                "grep" => tools.push(Box::new(GrepTool::new(cwd))),
                "find" => tools.push(Box::new(FindTool::new(cwd))),
                "ls" => tools.push(Box::new(LsTool::new(cwd))),
                _ => {}
            }
        }
        Self { tools }
    }

    pub fn from_tools(tools: Vec<Box<dyn Tool>>) -> Self {
        Self { tools }
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn push(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn extend<I: IntoIterator<Item = Box<dyn Tool>>>(&mut self, tools: I) {
        self.tools.extend(tools);
    }

    pub fn tools(&self) -> &[Box<dyn Tool>] {
        &self.tools
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|t| t.name() == name).map(|t| t.as_ref())
    }
}

// ============================================================================
// Path utilities
// ============================================================================

/// Resolve a relative or absolute path against the working directory.
pub fn resolve_path(path: &str, cwd: &Path) -> std::path::PathBuf {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// Canonicalize a path, resolving symlinks.
pub fn canonicalize_safe(path: &std::path::Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Ensure a path is within the CWD scope.
pub fn enforce_cwd_scope(path: &std::path::Path, cwd: &Path, tool: &str) -> Result<std::path::PathBuf> {
    let canonical = canonicalize_safe(path);
    let cwd_canonical = canonicalize_safe(cwd);

    if !canonical.starts_with(&cwd_canonical) {
        return Err(Error::tool(
            tool,
            format!(
                "Path '{}' is outside the working directory. All file access is restricted to the current project.",
                path.display()
            ),
        ));
    }
    Ok(canonical)
}

// ============================================================================
// Truncation constants
// ============================================================================

pub const DEFAULT_MAX_LINES: usize = 2000;
pub const DEFAULT_MAX_BYTES: usize = 1_000_000;
pub const GREP_MAX_LINE_LENGTH: usize = 500;
pub const DEFAULT_GREP_LIMIT: usize = 100;
pub const DEFAULT_FIND_LIMIT: usize = 1000;
pub const DEFAULT_LS_LIMIT: usize = 500;
pub const LS_SCAN_HARD_LIMIT: usize = 20_000;
pub const READ_TOOL_MAX_BYTES: u64 = 100 * 1024 * 1024;
pub const WRITE_TOOL_MAX_BYTES: usize = 100 * 1024 * 1024;
pub const IMAGE_MAX_BYTES: usize = 4_718_592;
pub const DEFAULT_BASH_TIMEOUT_SECS: u64 = 120;

/// Truncate output to max_bytes with a truncation notice.
pub fn truncate_output(output: &str, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output.to_string();
    }
    let boundary = max_bytes.saturating_sub(80);
    if boundary == 0 {
        return format!("... [truncated, original {} bytes]", output.len());
    }
    let truncated = &output[..output.floor_char_boundary(boundary)];
    format!("{truncated}\n... [truncated, original {} bytes]", output.len())
}

/// Truncate output by lines.
pub fn truncate_by_lines(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }
    let kept: Vec<&str> = lines.iter().take(max_lines).copied().collect();
    format!(
        "{}\n... [truncated, {} lines total]",
        kept.join("\n"),
        lines.len()
    )
}
