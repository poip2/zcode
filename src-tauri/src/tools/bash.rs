//! Bash tool — executes shell commands.
//! Adapted from pi-agent-rust (src/tools.rs).

use crate::error::{Error, Result};
use crate::model::{ContentBlock, TextContent};
use crate::tools::{
    truncate_by_lines, truncate_output, Tool, ToolEffects, ToolOutput, ToolUpdate,
    DEFAULT_BASH_TIMEOUT_SECS,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStderr, ChildStdout, Command as TokioCommand};
use tokio::time::timeout;

/// Input parameters for the bash tool.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BashInput {
    command: String,
    timeout: Option<u64>,
    success_exit_codes: Option<Vec<i32>>,
}

const SHELL_MAX_LINES: usize = 200;
const SHELL_MAX_BYTES: usize = 30_000;
const CAPTURE_NOTICE_RESERVE: usize = 96;

struct BoundedCapture {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    total_bytes: usize,
    head_limit: usize,
    tail_limit: usize,
}

impl BoundedCapture {
    fn new(max_bytes: usize) -> Self {
        let data_limit = max_bytes.saturating_sub(CAPTURE_NOTICE_RESERVE);
        let head_limit = data_limit / 2;
        Self {
            head: Vec::with_capacity(head_limit),
            tail: VecDeque::with_capacity(data_limit - head_limit),
            total_bytes: 0,
            head_limit,
            tail_limit: data_limit - head_limit,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(chunk.len());

        let head_remaining = self.head_limit.saturating_sub(self.head.len());
        let head_count = head_remaining.min(chunk.len());
        self.head.extend_from_slice(&chunk[..head_count]);

        let remaining = &chunk[head_count..];
        if remaining.is_empty() || self.tail_limit == 0 {
            return;
        }
        if remaining.len() >= self.tail_limit {
            self.tail.clear();
            self.tail.extend(
                remaining[remaining.len() - self.tail_limit..]
                    .iter()
                    .copied(),
            );
            return;
        }

        let overflow = self
            .tail
            .len()
            .saturating_add(remaining.len())
            .saturating_sub(self.tail_limit);
        if overflow > 0 {
            self.tail.drain(..overflow);
        }
        self.tail.extend(remaining.iter().copied());
    }

    fn into_string(self) -> String {
        let retained = self.head.len() + self.tail.len();
        let omitted = self.total_bytes.saturating_sub(retained);
        let mut bytes = Vec::with_capacity(SHELL_MAX_BYTES);
        bytes.extend_from_slice(&self.head);
        if omitted > 0 {
            bytes.extend_from_slice(
                format!("\n... [truncated, {omitted} bytes omitted] ...\n").as_bytes(),
            );
        }
        bytes.extend(self.tail);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<String> {
    let mut capture = BoundedCapture::new(SHELL_MAX_BYTES);
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        capture.push(&buffer[..read]);
    }
    Ok(capture.into_string())
}

async fn wait_with_bounded_output(
    child: &mut Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
) -> std::io::Result<(ExitStatus, String, String)> {
    let (status, stdout, stderr) =
        tokio::join!(child.wait(), read_bounded(stdout), read_bounded(stderr));
    Ok((status?, stdout?, stderr?))
}

pub struct BashTool {
    cwd: PathBuf,
    augmented_path: Option<String>,
    venv_dir: Option<PathBuf>,
}

impl BashTool {
    pub fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            augmented_path: None,
            venv_dir: None,
        }
    }

    pub fn with_runtime(cwd: &Path, augmented_path: String, venv_dir: PathBuf) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            augmented_path: Some(augmented_path),
            venv_dir: Some(venv_dir),
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
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use crate::tools::CREATE_NO_WINDOW;

/// Kill a process tree. On Windows uses taskkill /T, on Unix uses kill -9 -pid.
#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

#[cfg(not(windows))]
fn kill_process_tree(pid: u32) {
    // Negative pid targets the whole process group created via `.process_group(0)`.
    let _ = std::process::Command::new("kill")
        .args(["-9", &format!("-{pid}")])
        .status();
}

#[cfg(windows)]
fn spawn_shell(
    cwd: &Path,
    command: &str,
    augmented_path: Option<&str>,
    venv_dir: Option<&Path>,
) -> std::io::Result<Child> {
    let wrapped = wrap_windows_command(command);
    let mut last_err = None;
    for shell in WINDOWS_SHELLS {
        let mut cmd = TokioCommand::new(shell);
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &wrapped])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(path) = augmented_path {
            cmd.env("PATH", path);
        }
        if let Some(venv) = venv_dir {
            cmd.env("VIRTUAL_ENV", venv.to_str().unwrap_or(""));
        }
        match cmd.spawn() {
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
fn spawn_shell(
    cwd: &Path,
    command: &str,
    augmented_path: Option<&str>,
    venv_dir: Option<&Path>,
) -> std::io::Result<Child> {
    let mut cmd = TokioCommand::new("bash");
    cmd.arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // New process group so a timeout kill takes out the whole subtree
        // (pipelines, backgrounded children), not just the bash process.
        .process_group(0);
    if let Some(path) = augmented_path {
        cmd.env("PATH", path);
    }
    if let Some(venv) = venv_dir {
        cmd.env("VIRTUAL_ENV", venv.to_str().unwrap_or(""));
    }
    cmd.spawn()
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
        if self.augmented_path.is_some() {
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
                 Returns stdout and stderr. Output keeps bounded head and tail sections: up to \
                 200 retained lines plus a truncation notice, with a hard 30KB ceiling. \
                 Optionally provide a timeout in seconds. A bundled, isolated Python (managed by uv) and Bun runtime are available on \
                 PATH: use `uv pip install <package>` (not `pip install`) for Python \
                 packages, and `bun add` / `bun run` (not `npm` / `node`) for \
                 JavaScript/TypeScript."
            } else {
                "Execute a bash command in the current working directory. Use for directory \
                 listing and path/content search as well as project commands. Optional search \
                 commands may be absent: check `command -v rg`, then fall back to portable \
                 `find`/`grep`; on macOS avoid GNU-only flags. Returns stdout and stderr. Output \
                 keeps bounded head and tail sections: up to 200 retained lines plus a truncation \
                 notice, with a hard 30KB ceiling. Optionally provide a timeout in seconds. \
                 For searches, pass `successExitCodes: [0, 1]`; exit code 1 means no matches. \
                 A bundled, isolated Python (managed by uv) and Bun runtime are available on \
                 PATH: use `uv pip install <package>` (not `pip install`) for Python \
                 packages, and `bun add` / `bun run` (not `npm` / `node`) for \
                 JavaScript/TypeScript."
            }
        } else if cfg!(windows) {
            "Execute a shell command in the current working directory. Runs via PowerShell on \
             Windows (pwsh if available, else Windows PowerShell) — use PowerShell syntax, not \
             bash. Examples:\n\
             - list files incl. hidden: Get-ChildItem -Force\n\
             - find by name: Get-ChildItem -Recurse -Filter *.py\n\
             - grep-like search: Select-String -Path *.txt -Pattern 'TODO'\n\
             - filter processes: Get-Process | Where-Object { $_.ProcessName -like '*python*' }\n\
             - set env var: $env:FOO = 'bar'\n\
             - chain commands: use ; not &&\n\
             Returns stdout and stderr. Output keeps bounded head and tail sections: up to 200 \
             retained lines plus a truncation notice, with a hard 30KB ceiling. Optionally \
             provide a timeout in seconds."
        } else {
            "Execute a bash command in the current working directory. Use for directory listing \
             and path/content search as well as project commands. Optional search commands may \
             be absent: check `command -v rg`, then fall back to portable `find`/`grep`; on macOS \
             avoid GNU-only flags. Returns stdout and stderr. Output keeps bounded head and tail \
             sections: up to 200 retained lines plus a truncation notice, with a hard 30KB \
             ceiling. Optionally provide a timeout in seconds. For searches, pass \
             `successExitCodes: [0, 1]`; exit code 1 means no matches."
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
                },
                "successExitCodes": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Exit codes treated as success (default: [0]). For grep/rg searches use [0, 1], where 1 means no matches."
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

        let mut child = spawn_shell(
            &self.cwd,
            &input.command,
            self.augmented_path.as_deref(),
            self.venv_dir.as_deref(),
        )
        .map_err(|e| Error::tool("shell", format!("Failed to execute command: {e}")))?;

        let pid = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::tool("shell", "Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::tool("shell", "Failed to capture stderr"))?;
        let output_future = wait_with_bounded_output(&mut child, stdout, stderr);
        let (status, stdout, stderr) = if timeout_secs > 0 {
            match timeout(Duration::from_secs(timeout_secs), output_future).await {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => return Err(Error::tool("shell", format!("Command failed: {e}"))),
                Err(_) => {
                    if let Some(pid) = pid {
                        kill_process_tree(pid);
                    }
                    let _ = child.wait().await;
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

        let combined = truncate_output(&combined, SHELL_MAX_BYTES);
        let combined = truncate_by_lines(&combined, SHELL_MAX_LINES);
        // Line truncation adds its own notice; enforce byte ceiling again afterward.
        let combined = truncate_output(&combined, SHELL_MAX_BYTES);

        let exit_code = status.code().unwrap_or(-1);
        let is_error = !input
            .success_exit_codes
            .as_deref()
            .unwrap_or(&[0])
            .contains(&exit_code);
        let details = serde_json::json!({
            "exitCode": exit_code,
            "timeout": input.timeout,
            "successExitCodes": input.success_exit_codes,
        });

        Ok(ToolOutput {
            content: vec![ContentBlock::Text(TextContent::new(combined))],
            details: Some(details),
            is_error,
        })
    }
}
