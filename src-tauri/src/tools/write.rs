//! Write tool — creates or overwrites files.
//! Adapted from pi-agent-rust (src/tools.rs).

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{Tool, ToolEffects, ToolOutput, ToolUpdate, WRITE_TOOL_MAX_BYTES, resolve_path, enforce_cwd_scope};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Input parameters for the write tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteInput {
    path: String,
    content: String,
}

pub struct WriteTool {
    cwd: PathBuf,
}

impl WriteTool {
    pub fn new(cwd: &Path) -> Self {
        Self { cwd: cwd.to_path_buf() }
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str { "write" }
    fn label(&self) -> &str { "write" }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. \
         Automatically creates parent directories."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write (relative or absolute)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::write()
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        input: serde_json::Value,
        _on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    ) -> Result<ToolOutput> {
        let write_input: WriteInput =
            serde_json::from_value(input).map_err(|e| Error::validation(e.to_string()))?;

        if write_input.content.len() > WRITE_TOOL_MAX_BYTES {
            return Err(Error::validation(format!(
                "Content size exceeds maximum allowed ({} > {WRITE_TOOL_MAX_BYTES} bytes)",
                write_input.content.len()
            )));
        }

        let path = resolve_path(&write_input.path, &self.cwd);
        let path = enforce_cwd_scope(&path, &self.cwd, "write")?;

        // Check if target exists and is a directory
        if let Ok(meta) = tokio::fs::metadata(&path).await {
            if meta.is_dir() {
                return Err(Error::tool(
                    "write",
                    format!("Path '{}' is a directory, not a file", write_input.path),
                ));
            }
        }

        // Create parent directories
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Error::tool("write", format!("Failed to create parent directories: {e}"))
            })?;
        }

        tokio::fs::write(&path, &write_input.content).await.map_err(|e| {
            Error::tool("write", format!("Failed to write file: {e}"))
        })?;

        let msg = format!(
            "Successfully wrote {} bytes to '{}'.",
            write_input.content.len(),
            write_input.path
        );

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(msg))],
            details: None,
            is_error: false,
        })
    }
}
