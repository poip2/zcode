//! Find tool — search for files by glob pattern using fd-find.
//! Adapted from pi-agent-rust (src/tools.rs).
//!
//! Requires `fd` (fd-find) to be installed and available in PATH.

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    enforce_cwd_scope, resolve_path, truncate_output, Tool, ToolEffects, ToolOutput, ToolUpdate,
    DEFAULT_FIND_LIMIT, DEFAULT_MAX_BYTES,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command as TokioCommand;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use crate::tools::CREATE_NO_WINDOW;

/// Input parameters for the find tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindInput {
    pattern: String,
    path: Option<String>,
    limit: Option<usize>,
}

pub struct FindTool {
    cwd: PathBuf,
    allowed_roots: Vec<PathBuf>,
}

impl FindTool {
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

fn find_fd_binary() -> Option<&'static str> {
    static BINARY: OnceLock<Option<&'static str>> = OnceLock::new();
    *BINARY.get_or_init(|| {
        ["fd", "fdfind"]
            .iter()
            .find(|name| {
                let mut cmd = std::process::Command::new(name);
                cmd.arg("--version")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                #[cfg(windows)]
                cmd.creation_flags(CREATE_NO_WINDOW);
                cmd.status().is_ok()
            })
            .copied()
    })
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }
    fn label(&self) -> &str {
        "find"
    }

    fn description(&self) -> &str {
        "Search for files by glob pattern. Returns matching file paths relative to the search \
         directory. Sorted by modification time (newest first). Respects .gitignore. Output is \
         truncated to 1000 results or 50KB (whichever is hit first)."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files, e.g. '*.ts', '**/*.json', 'src/**/*.rs'"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in (default: current directory)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 1000)"
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
        let find_input: FindInput =
            serde_json::from_value(input).map_err(|e| Error::validation(e.to_string()))?;

        if matches!(find_input.limit, Some(0)) {
            return Err(Error::validation("`limit` must be greater than 0"));
        }

        let fd_cmd = find_fd_binary().ok_or_else(|| {
            Error::tool(
                "find",
                "fd is not available. Please install fd-find (fd) to use the find tool.",
            )
        })?;

        let search_dir = if let Some(ref p) = find_input.path {
            resolve_path(p, &self.cwd)
        } else {
            self.cwd.clone()
        };
        let search_dir = enforce_cwd_scope(&search_dir, &self.cwd, &self.allowed_roots, "find")?;

        if !search_dir.exists() {
            return Err(Error::tool(
                "find",
                format!("Path not found: {}", search_dir.display()),
            ));
        }

        let effective_limit = find_input.limit.unwrap_or(DEFAULT_FIND_LIMIT);
        // Fetch extra to detect truncation
        let scan_limit = effective_limit.saturating_add(1);

        let mut args: Vec<String> = vec![
            "--glob".into(),
            find_input.pattern.clone(),
            "--color=never".into(),
            "--hidden".into(),
            "--max-results".into(),
            scan_limit.to_string(),
        ];

        args.push("--".into());
        args.push(search_dir.display().to_string());

        let mut cmd = TokioCommand::new(fd_cmd);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd
            .output()
            .await
            .map_err(|e| Error::tool("find", format!("Failed to run fd: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if stdout.is_empty() && !stderr.is_empty() {
            return Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new(format!(
                    "(no results for '{}')\n\nstderr: {stderr}",
                    find_input.pattern,
                )))],
                details: None,
                is_error: false,
            });
        }

        if stdout.is_empty() {
            return Ok(ToolOutput {
                content: vec![ContentBlock::Text(TextContent::new(format!(
                    "(no results for '{}')",
                    find_input.pattern,
                )))],
                details: None,
                is_error: false,
            });
        }

        let lines: Vec<&str> = stdout.lines().collect();
        let total = lines.len();
        let truncated = total > effective_limit;
        let results: Vec<&str> = lines.into_iter().take(effective_limit).collect();

        let mut result = String::new();
        for line in &results {
            result.push_str(line);
            result.push('\n');
        }
        if truncated {
            result.push_str(&format!(
                "\n... [truncated, {} total results for '{}']",
                total, find_input.pattern,
            ));
        }

        let result = truncate_output(&result, DEFAULT_MAX_BYTES);

        let header = format!(
            "Found {} file(s) matching '{}':\n\n",
            std::cmp::min(total, effective_limit),
            find_input.pattern,
        );

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(format!(
                "{header}{result}"
            )))],
            details: None,
            is_error: false,
        })
    }
}
