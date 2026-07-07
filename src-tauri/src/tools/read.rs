//! Read tool — reads file contents with offset/limit support.
//! Adapted from pi-agent-rust (src/tools.rs).

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    Tool, ToolEffects, ToolOutput, ToolUpdate, READ_TOOL_MAX_BYTES, resolve_path, enforce_cwd_scope,
    truncate_output, truncate_by_lines, DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Input parameters for the read tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadInput {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

pub struct ReadTool {
    cwd: PathBuf,
}

impl ReadTool {
    pub fn new(cwd: &Path) -> Self {
        Self { cwd: cwd.to_path_buf() }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str { "read" }
    fn label(&self) -> &str { "read" }

    fn description(&self) -> &str {
        "Read the contents of a file. Supports text files and images (jpg, png, gif, webp, bmp). \
         Images are sent as attachments. For text files, output is truncated to 2000 lines or \
         50KB (whichever is hit first). Use offset/limit for large files. When you need the full \
         file, continue with offset until complete."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read (relative or absolute)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["path"]
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
        let input: ReadInput =
            serde_json::from_value(input).map_err(|e| Error::validation(e.to_string()))?;

        if matches!(input.limit, Some(l) if l == 0) {
            return Err(Error::validation("`limit` must be greater than 0"));
        }

        let path = resolve_path(&input.path, &self.cwd);
        let path = enforce_cwd_scope(&path, &self.cwd, "read")?;

        let meta = tokio::fs::metadata(&path).await.map_err(|e| {
            Error::tool("read", format!("Cannot access file '{}': {e}", input.path))
        })?;

        if !meta.is_file() {
            return Err(Error::tool("read", format!("Path '{}' is not a regular file", input.path)));
        }

        if meta.len() > READ_TOOL_MAX_BYTES {
            return Err(Error::tool(
                "read",
                format!("File is too large ({} bytes). Max allowed is {} bytes.", meta.len(), READ_TOOL_MAX_BYTES),
            ));
        }

        // Check for image files
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let is_image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp");
        if is_image {
            let bytes = tokio::fs::read(&path).await.map_err(|e| {
                Error::tool("read", format!("Failed to read image: {e}"))
            })?;
            let mime = if ext == "jpg" || ext == "jpeg" {
                "image/jpeg".to_string()
            } else {
                format!("image/{ext}")
            };
            let base64 = base64_encode(&bytes);
            return Ok(ToolOutput {
                content: vec![
                    ContentBlock::Text(TextContent::new(format!("Read image file [{mime}]"))),
                    ContentBlock::Image(crate::model::ImageContent {
                        data: base64,
                        mime_type: mime,
                    }),
                ],
                details: None,
                is_error: false,
            });
        }

        // Read text content
        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            Error::tool("read", format!("Failed to read file: {e}"))
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let offset = input.offset.unwrap_or(1).saturating_sub(1);
        let limit = input.limit.unwrap_or(DEFAULT_MAX_LINES);

        if offset >= lines.len() {
            return Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new(""))],
                details: None,
                is_error: false,
            });
        }

        let selected: Vec<&str> = lines.iter().skip(offset).take(limit).copied().collect();
        let output = selected.join("\n");
        let output = truncate_output(&output, DEFAULT_MAX_BYTES);
        let output = truncate_by_lines(&output, DEFAULT_MAX_LINES);

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(output))],
            details: None,
            is_error: false,
        })
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
