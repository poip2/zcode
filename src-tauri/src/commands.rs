use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn read_markdown_file(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("File not found: {}", path));
    }
    if !p.is_file() {
        return Err(format!("Not a file: {}", path));
    }
    fs::read_to_string(p).map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub fn write_markdown_file(path: String, content: String) -> Result<(), String> {
    let p = Path::new(&path);
    if p.exists() && !p.is_file() {
        return Err(format!("Not a file: {}", path));
    }
    fs::write(p, content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
pub fn resolve_path(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let absolute = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Failed to determine current directory: {}", e))?
            .join(p)
    };
    absolute
        .canonicalize()
        .unwrap_or(absolute)
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path))
}

#[tauri::command]
pub fn allow_assets(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    let scope = app.asset_protocol_scope();
    for p in &paths {
        scope
            .allow_file(p)
            .map_err(|e| format!("Failed to allow asset {}: {}", p, e))?;
    }
    Ok(())
}
