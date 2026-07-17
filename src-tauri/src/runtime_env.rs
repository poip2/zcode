use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone)]
pub struct AgentRuntime {
    pub venv_dir: PathBuf,
    pub bun_bin_dir: PathBuf,
}

pub struct RuntimeState {
    pub runtime: std::sync::Mutex<Option<AgentRuntime>>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            runtime: std::sync::Mutex::new(None),
        }
    }
}

fn resource_bin(app: &AppHandle, rel: &str) -> Result<PathBuf, String> {
    app.path()
        .resolve(
            format!("resources/runtime/{rel}"),
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| e.to_string())
}

fn embedded_python(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    return resource_bin(app, "python/python.exe");
    #[cfg(not(target_os = "windows"))]
    return resource_bin(app, "python/bin/python3");
}

fn embedded_uv(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    return resource_bin(app, "bin/uv.exe");
    #[cfg(not(target_os = "windows"))]
    return resource_bin(app, "bin/uv");
}

fn embedded_bun_dir(app: &AppHandle) -> Result<PathBuf, String> {
    resource_bin(app, "bin")
}

/// 幂等：venv 不存在才创建。用 uv venv，不用 python -m venv。
pub async fn ensure_agent_venv(app: &AppHandle) -> Result<AgentRuntime, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let venv_dir = app_dir.join("agent_venv");
    let bun_bin_dir = embedded_bun_dir(app)?;

    let python_bin = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python3")
    };
    let sentinel = venv_dir.join("pyvenv.cfg");
    let venv_valid = sentinel.exists() && python_bin.exists();

    if !venv_valid {
        if venv_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&venv_dir) {
                eprintln!("[zcode] Failed to remove broken venv: {e}");
            }
        }
        let uv = embedded_uv(app)?;
        let python = embedded_python(app)?;

        let output = tokio::time::timeout(Duration::from_secs(120), TokioCommand::new(&uv)
            .arg("venv")
            .arg(&venv_dir)
            .arg("--python")
            .arg(&python)
            .output())
            .await
            .map_err(|_| "内置运行时初始化超时（超过120秒），请检查系统资源后重试".to_string())?
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
    }

    Ok(AgentRuntime {
        venv_dir,
        bun_bin_dir,
    })
}

/// 生成注入了内置运行时的 PATH，给 bash tool 的 spawn_shell 用。
pub fn augmented_path(runtime: &AgentRuntime) -> String {
    #[cfg(target_os = "windows")]
    let venv_bin = runtime.venv_dir.join("Scripts");
    #[cfg(not(target_os = "windows"))]
    let venv_bin = runtime.venv_dir.join("bin");

    let original = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };

    format!(
        "{}{sep}{}{sep}{}",
        venv_bin.display(),
        runtime.bun_bin_dir.display(),
        original
    )
}
