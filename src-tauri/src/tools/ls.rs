//! Ls tool — list directory contents.
//! Adapted from pi-agent-rust (src/tools.rs).

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    Tool, ToolEffects, ToolOutput, ToolUpdate, DEFAULT_LS_LIMIT, LS_SCAN_HARD_LIMIT,
    DEFAULT_MAX_BYTES, resolve_path, enforce_cwd_scope, truncate_output,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Input parameters for the ls tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LsInput {
    path: Option<String>,
    limit: Option<usize>,
}

pub struct LsTool {
    cwd: PathBuf,
}

impl LsTool {
    pub fn new(cwd: &Path) -> Self {
        Self { cwd: cwd.to_path_buf() }
    }
}

#[async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str { "ls" }
    fn label(&self) -> &str { "ls" }

    fn description(&self) -> &str {
        "List directory contents. Returns entries sorted alphabetically, with '/' suffix for \
         directories. Includes dotfiles. Output is truncated to 500 entries or 50KB (whichever \
         is hit first)."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list (default: current directory)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of entries to return (default: 500)"
                }
            }
        })
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::read()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        input: serde_json::Value,
        _on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    ) -> Result<ToolOutput> {
        let ls_input: LsInput =
            serde_json::from_value(input).map_err(|e| Error::validation(e.to_string()))?;

        if matches!(ls_input.limit, Some(0)) {
            return Err(Error::validation("`limit` must be greater than 0"));
        }

        let dir_path = ls_input.path.as_ref().map_or_else(
            || self.cwd.clone(),
            |p| resolve_path(p, &self.cwd),
        );
        let dir_path = enforce_cwd_scope(&dir_path, &self.cwd, "ls")?;

        if !dir_path.exists() {
            return Err(Error::tool("ls", format!("Path not found: {}", dir_path.display())));
        }
        if !dir_path.is_dir() {
            return Err(Error::tool("ls", format!("Not a directory: {}", dir_path.display())));
        }

        let effective_limit = ls_input.limit.unwrap_or(DEFAULT_LS_LIMIT);

        let mut read_dir = tokio::fs::read_dir(&dir_path).await.map_err(|e| {
            Error::tool("ls", format!("Cannot read directory: {e}"))
        })?;

        let mut entries: Vec<(String, bool)> = Vec::new();

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| {
            Error::tool("ls", format!("Cannot read directory entry: {e}"))
        })? {
            if entries.len() >= LS_SCAN_HARD_LIMIT {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip . and ..
            if name == "." || name == ".." {
                continue;
            }
            // Use file_type to check if it's a directory without extra stat
            let is_dir = entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false);
            entries.push((name, is_dir));
        }

        // Sort alphabetically (case-insensitive)
        entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        if entries.is_empty() {
            return Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new("(empty directory)"))],
                details: None,
                is_error: false,
            });
        }

        let total = entries.len();
        let truncated = total > effective_limit;
        entries.truncate(effective_limit);

        let mut output = String::new();
        for (name, is_dir) in &entries {
            if *is_dir {
                output.push_str(&format!("{name}/\n"));
            } else {
                output.push_str(&format!("{name}\n"));
            }
        }

        if truncated {
            output.push_str(&format!(
                "\n... [truncated, {} total entries in '{}']",
                total,
                dir_path.display(),
            ));
        }

        let output = truncate_output(&output, DEFAULT_MAX_BYTES);
        let header = format!(
            "{} entries in '{}':\n\n",
            std::cmp::min(total, effective_limit),
            dir_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| dir_path.to_string_lossy()),
        );

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(format!("{header}{output}")))],
            details: serde_json::json!({ "count": total }).into(),
            is_error: false,
        })
    }
}
