//! Grep tool — search file contents using ripgrep.
//! Adapted from pi-agent-rust (src/tools.rs).
//!
//! Requires `rg` (ripgrep) to be installed and available in PATH.

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    enforce_cwd_scope, resolve_path, truncate_at_char_boundary, truncate_output, Tool, ToolEffects,
    ToolOutput, ToolUpdate, DEFAULT_GREP_LIMIT, DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command as TokioCommand;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Input parameters for the grep tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrepInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    ignore_case: Option<bool>,
    literal: Option<bool>,
    context: Option<usize>,
    limit: Option<usize>,
}

pub struct GrepTool {
    cwd: PathBuf,
}

impl GrepTool {
    pub fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
        }
    }
}

fn find_rg_binary() -> Option<&'static str> {
    static BINARY: OnceLock<Option<&'static str>> = OnceLock::new();
    *BINARY.get_or_init(|| {
        ["rg", "ripgrep"]
            .iter()
            .find(|name| {
                let mut cmd = std::process::Command::new(name);
                cmd.arg("--version").stdout(Stdio::null()).stderr(Stdio::null());
                #[cfg(windows)]
                cmd.creation_flags(0x08000000);
                cmd.status().is_ok()
            })
            .copied()
    })
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn label(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents for a pattern. Returns matching lines with file paths and line numbers. \
         Respects .gitignore. Output is truncated to 50KB and at most 100 matches per file. \
         Long lines are truncated to 500 chars."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Pattern to search for (regex by default, literal if literal:true)"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search (default: current directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs', '**/*.ts')"
                },
                "ignoreCase": {
                    "type": "boolean",
                    "description": "Case-insensitive search"
                },
                "literal": {
                    "type": "boolean",
                    "description": "Treat pattern as literal string instead of regex"
                },
                "context": {
                    "type": "integer",
                    "description": "Number of context lines to show around each match"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of matches (default: 100)"
                }
            },
            "required": ["pattern"]
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
        let grep_input: GrepInput =
            serde_json::from_value(input).map_err(|e| Error::validation(e.to_string()))?;

        let rg_cmd = find_rg_binary().ok_or_else(|| {
            Error::tool(
                "grep",
                "rg is not available. Please install ripgrep (rg) to use the grep tool.",
            )
        })?;

        let limit = grep_input.limit.unwrap_or(DEFAULT_GREP_LIMIT);
        let search_path = if let Some(ref p) = grep_input.path {
            resolve_path(p, &self.cwd)
        } else {
            self.cwd.clone()
        };
        let search_path = enforce_cwd_scope(&search_path, &self.cwd, "grep")?;

        let mut args: Vec<String> = vec![
            "--line-number".into(),
            "--no-heading".into(),
            "--color=never".into(),
            "--no-messages".into(),
            "--max-count".into(),
            limit.to_string(),
        ];

        if grep_input.ignore_case.unwrap_or(false) {
            args.push("--ignore-case".into());
        }
        if grep_input.literal.unwrap_or(false) {
            args.push("--fixed-strings".into());
        }
        if let Some(ctx) = grep_input.context {
            if ctx > 0 {
                args.push("--context".into());
                args.push(ctx.to_string());
            }
        }
        if let Some(ref glob) = grep_input.glob {
            args.push("--glob".into());
            args.push(glob.clone());
        }

        args.push("--".into());
        args.push(grep_input.pattern.clone());
        args.push(search_path.display().to_string());

        let mut cmd = TokioCommand::new(rg_cmd);
        cmd.args(&args)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let output = cmd
            .output()
            .await
            .map_err(|e| Error::tool("grep", format!("Failed to run ripgrep: {e}")))?;

        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if stdout.is_empty() && !stderr.is_empty() {
            return Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new(format!(
                    "(no matches for '{}')\n\nstderr: {stderr}",
                    truncate_for_display(&grep_input.pattern),
                )))],
                details: None,
                is_error: false,
            });
        }

        if stdout.is_empty() {
            return Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new(format!(
                    "(no matches for '{}')",
                    truncate_for_display(&grep_input.pattern),
                )))],
                details: None,
                is_error: false,
            });
        }

        // Truncate long lines
        let lines: Vec<String> = stdout
            .lines()
            .map(|line| {
                if line.len() > GREP_MAX_LINE_LENGTH {
                    format!(
                        "{}... [truncated]",
                        truncate_at_char_boundary(line, GREP_MAX_LINE_LENGTH)
                    )
                } else {
                    line.to_string()
                }
            })
            .collect();
        stdout = lines.join("\n");

        let line_count = stdout.lines().count();
        let stdout = truncate_output(&stdout, DEFAULT_MAX_BYTES);

        let header = format!(
            "Found {} matching line(s) for '{}':\n\n",
            line_count,
            truncate_for_display(&grep_input.pattern),
        );

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(format!(
                "{header}{stdout}"
            )))],
            details: None,
            is_error: false,
        })
    }
}

fn truncate_for_display(s: &str) -> String {
    if s.len() <= 80 {
        s.to_string()
    } else {
        format!("{}...", truncate_at_char_boundary(s, 77))
    }
}
