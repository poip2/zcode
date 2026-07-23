//! Edit tool — precise text replacement in files.
//! Adapted from pi-agent-rust (src/tools.rs).

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    enforce_cwd_scope, resolve_path, Tool, ToolEffects, ToolOutput, ToolUpdate,
    READ_TOOL_MAX_BYTES, WRITE_TOOL_MAX_BYTES,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

pub struct EditTool {
    cwd: PathBuf,
    allowed_roots: Vec<PathBuf>,
}

impl EditTool {
    pub fn new(cwd: &Path) -> Self {
        Self::with_allowed_roots(cwd, Vec::new())
    }

    pub fn with_allowed_roots(cwd: &Path, allowed_roots: Vec<PathBuf>) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            allowed_roots,
        }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn label(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing text. The oldText must match a unique region; matching is exact. \
         Supports multiple disjoint edits in one call when edits[] is provided."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit (relative or absolute)"
                },
                "oldText": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Text to find and replace (must match uniquely)"
                },
                "newText": {
                    "type": "string",
                    "description": "New text to replace the old text with"
                },
                "edits": {
                    "type": "array",
                    "description": "Multiple edits to apply. Each has oldText and newText. Use instead of oldText/newText for multiple replacements.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "oldText": { "type": "string" },
                            "newText": { "type": "string" }
                        },
                        "required": ["oldText", "newText"]
                    }
                }
            },
            "required": ["path"]
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
        let path_str = input["path"]
            .as_str()
            .ok_or_else(|| Error::validation("Missing 'path' parameter"))?;

        let absolute_path = resolve_path(path_str, &self.cwd);
        let absolute_path =
            enforce_cwd_scope(&absolute_path, &self.cwd, &self.allowed_roots, "edit")?;

        // Parse edits: support both single oldText/newText and edits[] array
        let mut edits: Vec<(String, String)> = Vec::new();

        if let Some(edits_array) = input["edits"].as_array() {
            for edit in edits_array {
                let old_text = edit["oldText"].as_str().unwrap_or("").to_string();
                let new_text = edit["newText"].as_str().unwrap_or("").to_string();
                if old_text.is_empty() {
                    return Err(Error::validation("Each edit must have non-empty oldText"));
                }
                if new_text.len() > WRITE_TOOL_MAX_BYTES {
                    return Err(Error::validation(format!(
                        "New text size exceeds maximum ({} > {WRITE_TOOL_MAX_BYTES})",
                        new_text.len()
                    )));
                }
                edits.push((old_text, new_text));
            }
        } else {
            let old_text = input["oldText"].as_str().unwrap_or("").to_string();
            let new_text = input["newText"].as_str().unwrap_or("").to_string();
            if old_text.is_empty() {
                return Err(Error::validation("Must provide oldText or edits[]"));
            }
            if new_text.len() > WRITE_TOOL_MAX_BYTES {
                return Err(Error::validation(format!(
                    "New text size exceeds maximum ({} > {WRITE_TOOL_MAX_BYTES})",
                    new_text.len()
                )));
            }
            edits.push((old_text, new_text));
        }

        if edits.is_empty() {
            return Err(Error::validation("No edits provided"));
        }

        // Read file
        let meta = tokio::fs::metadata(&absolute_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::tool("edit", format!("File not found: {path_str}"))
            } else {
                Error::tool("edit", format!("Cannot access file: {e}"))
            }
        })?;

        if !meta.is_file() {
            return Err(Error::tool(
                "edit",
                format!("Path is not a regular file: {path_str}"),
            ));
        }
        if meta.len() > READ_TOOL_MAX_BYTES {
            return Err(Error::tool(
                "edit",
                format!("File too large ({} > {READ_TOOL_MAX_BYTES})", meta.len()),
            ));
        }

        let mut content = tokio::fs::read_to_string(&absolute_path)
            .await
            .map_err(|e| Error::tool("edit", format!("Failed to read file: {e}")))?;

        let original = content.clone();
        let mut positions: Vec<usize> = Vec::new();
        for (old_text, _new_text) in &edits {
            let matches: Vec<_> = original.match_indices(old_text.as_str()).collect();
            if matches.is_empty() {
                return Err(Error::tool(
                    "edit",
                    format!(
                        "oldText not found in file: '{}'",
                        truncate_for_error(old_text)
                    ),
                ));
            }
            if matches.len() > 1 {
                return Err(Error::tool(
                    "edit",
                    format!(
                        "oldText matches {} times — must be unique. Text: '{}'",
                        matches.len(),
                        truncate_for_error(old_text)
                    ),
                ));
            }
            positions.push(matches[0].0);
        }

        let mut sorted: Vec<(usize, &str, &str)> = positions
            .iter()
            .enumerate()
            .map(|(i, &pos)| (pos, edits[i].0.as_str(), edits[i].1.as_str()))
            .collect();
        sorted.sort_by_key(|(pos, _, _)| std::cmp::Reverse(*pos));

        for (pos, old_text, new_text) in &sorted {
            content.replace_range(*pos..pos + old_text.len(), new_text);
        }

        let total_replacements = edits.len();

        tokio::fs::write(&absolute_path, &content)
            .await
            .map_err(|e| Error::tool("edit", format!("Failed to write file: {e}")))?;

        let msg = format!(
            "Successfully applied {} edit(s) to '{}'. File size: {} bytes.",
            total_replacements,
            path_str,
            content.len()
        );

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(msg))],
            details: None,
            is_error: false,
        })
    }
}

fn truncate_for_error(s: &str) -> String {
    if s.len() <= 50 {
        s.to_string()
    } else {
        format!("{}...", crate::tools::truncate_at_char_boundary(s, 47))
    }
}
