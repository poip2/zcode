//! Bash tool — executes shell commands.
//! Adapted from pi-agent-rust (src/tools.rs).

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    truncate_by_lines, truncate_output, Tool, ToolEffects, ToolOutput, ToolUpdate,
    DEFAULT_BASH_TIMEOUT_SECS, DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

/// Input parameters for the bash tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BashInput {
    command: String,
    timeout: Option<u64>,
}

pub struct BashTool {
    cwd: PathBuf,
}

impl BashTool {
    pub fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn label(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command in the current working directory. Returns stdout and stderr. \
         Output is truncated to last 500 lines or 50KB (whichever is hit first). If truncated, \
         full output is saved to a temp file. Optionally provide a timeout in seconds."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Bash command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 120). Set 0 to disable."
                }
            },
            "required": ["command"]
        })
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::process().union(ToolEffects::write())
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        input: serde_json::Value,
        _on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    ) -> Result<ToolOutput> {
        let input: BashInput =
            serde_json::from_value(input).map_err(|e| Error::validation(e.to_string()))?;

        let timeout_secs = input.timeout.unwrap_or(DEFAULT_BASH_TIMEOUT_SECS);

        let child = TokioCommand::new("bash")
            .arg("-c")
            .arg(&input.command)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::tool("bash", format!("Failed to execute command: {e}")))?;

        let pid = child.id();
        let output_future = child.wait_with_output();
        let output = if timeout_secs > 0 {
            match timeout(Duration::from_secs(timeout_secs), output_future).await {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => return Err(Error::tool("bash", format!("Command failed: {e}"))),
                Err(_) => {
                    // Kill on timeout using stored PID
                    if let Some(pid) = pid {
                        let _ = if cfg!(unix) {
                            std::process::Command::new("kill")
                                .args(["-9", &pid.to_string()])
                                .status()
                        } else {
                            std::process::Command::new("taskkill")
                                .args(["/PID", &pid.to_string(), "/F"])
                                .status()
                        };
                    }
                    return Err(Error::tool(
                        "bash",
                        format!("Command timed out after {timeout_secs}s"),
                    ));
                }
            }
        } else {
            output_future
                .await
                .map_err(|e| Error::tool("bash", format!("Command failed: {e}")))?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut combined = String::new();
        if !stdout.is_empty() {
            combined.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str("--- stderr ---\n");
            combined.push_str(&stderr);
        }
        if combined.is_empty() {
            combined = "(no output)".to_string();
        }

        let combined = truncate_output(&combined, DEFAULT_MAX_BYTES);
        let combined = truncate_by_lines(&combined, DEFAULT_MAX_LINES);

        let is_error = !output.status.success();
        let details = serde_json::json!({
            "exitCode": output.status.code().unwrap_or(-1),
            "timeout": input.timeout,
        });

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(combined))],
            details: Some(details),
            is_error,
        })
    }
}
