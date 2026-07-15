use crate::model::{Message, UserContent, UserMessage};
use crate::provider::{Context, StreamOptions};
use crate::settings;
use futures::StreamExt;
use serde::Serialize;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

const MAX_TREE_DEPTH: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct DirNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DirNode>>,
}

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
    // Use dunce::canonicalize to avoid Windows \\?\ extended-length path prefix
    dunce::canonicalize(&absolute)
        .unwrap_or(absolute)
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path))
}

#[tauri::command]
pub fn path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
pub fn read_dir_tree(root: String) -> Result<DirNode, String> {
    let root_path = Path::new(&root);
    if !root_path.exists() {
        return Err(format!("Directory not found: {}", root));
    }
    if !root_path.is_dir() {
        return Err(format!("Not a directory: {}", root));
    }
    build_dir_node(root_path, 0).ok_or_else(|| format!("Failed to read directory: {}", root))
}

fn build_dir_node(dir: &Path, depth: u32) -> Option<DirNode> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // At max depth, show the directory but don't recurse into children
    if depth > MAX_TREE_DEPTH {
        return Some(DirNode {
            name,
            path: dir.to_string_lossy().to_string(),
            is_dir: true,
            modified: None,
            children: Some(vec![]),
        });
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return None,
    };
    let mut children: Vec<DirNode> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories
        if let Some(fname) = path.file_name() {
            if fname.to_string_lossy().starts_with('.') {
                continue;
            }
        }

        if path.is_dir() {
            // Skip common non-content directories
            if let Some(fname) = path.file_name() {
                let n = fname.to_string_lossy();
                if matches!(
                    n.as_ref(),
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | "__pycache__"
                        | "vendor"
                        | "zig-cache"
                        | "zig-out"
                ) {
                    continue;
                }
            }

            if let Some(subnode) = build_dir_node(&path, depth + 1) {
                children.push(subnode);
            }
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" || ext == "markdown" || ext == "mdown" || ext == "mkd" {
                    let fname = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64);

                    children.push(DirNode {
                        name: fname,
                        path: path.to_string_lossy().to_string(),
                        is_dir: false,
                        modified,
                        children: None,
                    });
                }
            }
        }
    }

    // Sort: directories first (alphabetically), then files (alphabetically)
    children.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let children = Some(children);

    Some(DirNode {
        name,
        path: dir.to_string_lossy().to_string(),
        is_dir: true,
        modified: None,
        children,
    })
}

#[tauri::command]
pub fn create_markdown_file(dir: String, name: String) -> Result<String, String> {
    let dir_path = Path::new(&dir);
    if !dir_path.is_dir() {
        return Err(format!("Not a directory: {}", dir));
    }

    let filename = if name.ends_with(".md")
        || name.ends_with(".markdown")
        || name.ends_with(".mdown")
        || name.ends_with(".mkd")
    {
        name
    } else {
        format!("{}.md", name)
    };

    validate_simple_name(&filename)?;
    let canonical_dir =
        dunce::canonicalize(dir_path).map_err(|e| format!("Failed to resolve directory: {}", e))?;
    let file_path = canonical_dir.join(&filename);

    if file_path.exists() {
        return Err(format!("File already exists: {}", file_path.display()));
    }

    fs::write(&file_path, "").map_err(|e| format!("Failed to create file: {}", e))?;

    file_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Path is not valid UTF-8".to_string())
}

#[tauri::command]
pub fn create_folder(dir: String, name: String) -> Result<String, String> {
    let dir_path = Path::new(&dir);
    if !dir_path.is_dir() {
        return Err(format!("Not a directory: {}", dir));
    }

    validate_simple_name(&name)?;
    let canonical_dir =
        dunce::canonicalize(dir_path).map_err(|e| format!("Failed to resolve directory: {}", e))?;
    let folder_path = canonical_dir.join(&name);

    if folder_path.exists() {
        return Err(format!("Folder already exists: {}", folder_path.display()));
    }

    fs::create_dir(&folder_path).map_err(|e| format!("Failed to create folder: {}", e))?;

    folder_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Path is not valid UTF-8".to_string())
}

fn validate_simple_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let p = Path::new(name);
    if p.file_name() != Some(p.as_os_str()) {
        return Err(
            "Name must be a simple file or folder name without directory components".to_string(),
        );
    }
    if name == "." || name == ".." {
        return Err("Invalid name".to_string());
    }
    Ok(())
}

// ============================================================================
// App paths
// ============================================================================

/// Return the app data directory so the frontend can compute default folder paths.
#[tauri::command]
pub fn get_app_data_dir(app: AppHandle) -> Result<String, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))?
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "App data dir is not valid UTF-8".to_string())
}

/// Open a file or folder path in the system file manager.
#[tauri::command]
pub fn open_in_shell(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    // Create the directory if it doesn't exist (for default output/pin folders)
    if !p.exists() {
        std::fs::create_dir_all(p)
            .map_err(|e| format!("Failed to create directory: {e}"))?;
    }
    open::that(p).map_err(|e| format!("Failed to open: {e}"))
}

// ============================================================================
// AI Provider Keychain Commands
// ============================================================================

/// Check whether an API key exists in the OS keychain.
///
/// This queries the real keychain state — unlike `maskedApiKey` in the
/// settings store, which is only a de-identified hint that can be stale.
#[tauri::command]
pub async fn check_api_key() -> Result<settings::ApiKeyStatus, String> {
    settings::check_api_key()
}

/// Store (or overwrite) the API key in the OS keychain.
/// Passing an empty string deletes the key.
/// Returns `Ok(None)` on success, `Ok(Some(warning))` if keychain unavailable.
#[tauri::command]
pub async fn save_api_key(api_key: String) -> Result<Option<String>, String> {
    if api_key.is_empty() {
        settings::delete_api_key()
    } else {
        settings::set_api_key(&api_key)
    }
}

/// Call the AI provider with a text prompt.
///
/// base_url + model come from the frontend (stored in the local store).
/// provider_name is an optional label for the provider (defaults to "openai").
/// apiKey is read from keychain internally — never returned to the caller.
#[tauri::command]
pub async fn call_ai_provider(
    base_url: String,
    model: String,
    prompt: String,
    provider_name: Option<String>,
) -> Result<String, String> {
    if base_url.is_empty() {
        return Err("No Base URL configured. Please set it in Settings > AI Provider.".to_string());
    }

    if model.is_empty() {
        return Err("No model configured. Please set it in Settings > AI Provider.".to_string());
    }
    let name = provider_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "openai".to_string());

    eprintln!(
        "[zcode] call_ai_provider: base_url={base_url}, model={model}, prompt_len={}",
        prompt.len()
    );

    let api_key = match settings::get_api_key() {
        Ok(Some(key)) => {
            eprintln!(
                "[zcode] call_ai_provider: API key found (len={})",
                key.len()
            );
            key
        }
        Ok(None) => {
            eprintln!("[zcode] call_ai_provider: ERROR no API key");
            return Err(
                "No API key configured. Please set it in Settings > AI Provider.".to_string(),
            );
        }
        Err(e) => {
            eprintln!("[zcode] call_ai_provider: ERROR reading keychain: {e}");
            return Err(e.to_string());
        }
    };

    let provider = crate::providers::build_provider(&name, &model, &api_key, &base_url)
        .map_err(|e| e.to_string())?;

    let user_msg = Message::User(UserMessage {
        content: UserContent::Text(prompt),
        timestamp: chrono::Utc::now().timestamp_millis(),
    });

    let context = Context {
        system_prompt: None,
        messages: &[user_msg],
        tools: &[],
    };

    let options = StreamOptions::default();

    let mut stream = provider.stream(&context, &options).await.map_err(|e| {
        eprintln!("[zcode] call_ai_provider: stream() FAILED: {e}");
        e.to_string()
    })?;

    eprintln!("[zcode] call_ai_provider: stream started, reading events...");
    let mut result_text = String::new();
    while let Some(event) = stream.next().await {
        match event {
            Ok(crate::model::StreamEvent::TextDelta { delta, .. }) => {
                result_text.push_str(&delta);
            }
            Ok(crate::model::StreamEvent::Error { error, .. }) => {
                eprintln!(
                    "[zcode] call_ai_provider: StreamEvent::Error {:?}",
                    error.error_message
                );
                if let Some(msg) = &error.error_message {
                    return Err(msg.clone());
                }
                return Err("Unknown AI provider error".to_string());
            }
            Ok(crate::model::StreamEvent::Done { message, .. }) => {
                eprintln!(
                    "[zcode] call_ai_provider: Done, text_len={}",
                    result_text.len()
                );
                // The provider may surface errors inside the Done event
                if let Some(msg) = &message.error_message {
                    return Err(msg.clone());
                }
            }
            Err(e) => {
                eprintln!("[zcode] call_ai_provider: stream Err: {e}");
                return Err(e.to_string());
            }
            _ => {}
        }
    }

    eprintln!(
        "[zcode] call_ai_provider: stream ended, result_len={}",
        result_text.len()
    );

    if result_text.is_empty() {
        return Err("AI provider returned an empty response.".to_string());
    }

    Ok(result_text)
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
