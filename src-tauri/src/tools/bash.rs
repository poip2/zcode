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
use tokio::process::{Child, Command as TokioCommand};
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

/// PowerShell binaries to try, in preference order. `pwsh` (PowerShell 7+)
/// defaults to UTF-8; `powershell.exe` (Windows PowerShell 5.1, ships on
/// every Windows box) is the fallback and needs an explicit UTF-8 override.
#[cfg(windows)]
const WINDOWS_SHELLS: [&str; 2] = ["pwsh", "powershell.exe"];

/// Wrap the user command so that:
/// - console I/O is forced to UTF-8 (Windows PowerShell 5.1 otherwise
///   decodes/encodes with the system code page and mangles non-ASCII text,
///   e.g. Chinese filenames/output)
/// - the PowerShell host exits with the same code as the last native
///   command it ran (`powershell -Command` otherwise returns 0 even if the
///   wrapped command failed, unless the script ends with an explicit `exit`)
#[cfg(windows)]
fn wrap_windows_command(command: &str) -> String {
    format!(
        "$OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n\
         [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n\
         {command}\n\
         if ($LASTEXITCODE -ne $null) {{ exit $LASTEXITCODE }}"
    )
}

#[cfg(windows)]
use std::io::ErrorKind;

#[cfg(windows)]
fn spawn_shell(cwd: &Path, command: &str) -> std::io::Result<Child> {
    let wrapped = wrap_windows_command(command);
    let mut last_err = None;
    for shell in WINDOWS_SHELLS {
        match TokioCommand::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", &wrapped])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => return Ok(child),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                last_err = Some(e);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            ErrorKind::NotFound,
            "neither pwsh nor powershell.exe found in PATH",
        )
    }))
}

#[cfg(not(windows))]
fn spawn_shell(cwd: &Path, command: &str) -> std::io::Result<Child> {
    TokioCommand::new("bash")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // New process group so a timeout kill takes out the whole subtree
        // (pipelines, backgrounded children), not just the bash process.
        .process_group(0)
        .spawn()
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn label(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        if cfg!(windows) {
            "Execute a shell command in the current working directory. Runs via PowerShell on \
             Windows (pwsh if available, else Windows PowerShell) — use PowerShell syntax, not \
             bash. Examples:\n\
             - list files incl. hidden: Get-ChildItem -Force\n\
             - find by name: Get-ChildItem -Recurse -Filter *.py\n\
             - grep-like search: Select-String -Path *.txt -Pattern 'TODO'\n\
             - filter processes: Get-Process | Where-Object { $_.ProcessName -like '*python*' }\n\
             - set env var: $env:FOO = 'bar'\n\
             - chain commands: use ; not &&\n\
             Returns stdout and stderr. Output is truncated to last 500 lines or 50KB \
             (whichever is hit first). If truncated, full output is saved to a temp file. \
             Optionally provide a timeout in seconds."
        } else {
            "Execute a bash command in the current working directory. Returns stdout and stderr. \
             Output is truncated to last 500 lines or 50KB (whichever is hit first). If \
             truncated, full output is saved to a temp file. Optionally provide a timeout in \
             seconds."
        }
    }

    fn parameters(&self) -> serde_json::Value {
        let command_desc = if cfg!(windows) {
            "PowerShell command to execute"
        } else {
            "Bash command to execute"
        };
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": command_desc
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

        let child = spawn_shell(&self.cwd, &input.command)
            .map_err(|e| Error::tool("shell", format!("Failed to execute command: {e}")))?;

        let pid = child.id();
        let output_future = child.wait_with_output();
        let output = if timeout_secs > 0 {
            match timeout(Duration::from_secs(timeout_secs), output_future).await {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => return Err(Error::tool("shell", format!("Command failed: {e}"))),
                Err(_) => {
                    if let Some(pid) = pid {
                        let _ = if cfg!(unix) {
                            // Negative pid targets the whole process group
                            // created via `.process_group(0)` above.
                            std::process::Command::new("kill")
                                .args(["-9", &format!("-{pid}")])
                                .status()
                        } else {
                            // /T kills the whole process tree, not just the
                            // PowerShell host.
                            std::process::Command::new("taskkill")
                                .args(["/PID", &pid.to_string(), "/T", "/F"])
                                .status()
                        };
                    }
                    return Err(Error::tool(
                        "shell",
                        format!("Command timed out after {timeout_secs}s"),
                    ));
                }
            }
        } else {
            output_future
                .await
                .map_err(|e| Error::tool("shell", format!("Command failed: {e}")))?
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
