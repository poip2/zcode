use crate::model::{Message, UserContent, UserMessage};
use crate::provider::{Context, StreamOptions};
use crate::settings;
use futures::StreamExt;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;
use tauri::{AppHandle, Manager};

const MAX_TREE_DEPTH: u32 = 3;

/// File extensions rendered as markdown by the frontend.
const MARKDOWN_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd"];

/// File extensions shown in the tree but opened externally (not rendered as markdown).
const DISPLAYABLE_EXTS: &[&str] = &[
    "docx", "doc", "xlsx", "xls", "pptx", "ppt", "pdf", "csv", "txt", "json", "xml", "yaml", "yml",
    "toml", "html",
];

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
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if MARKDOWN_EXTS.contains(&ext) || DISPLAYABLE_EXTS.contains(&ext) {
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

/// Resolve an existing entry without following the entry itself through a symlink.
/// Canonicalizing only the parent also gives us a stable absolute return path.
fn resolve_existing_entry(path: &str) -> Result<std::path::PathBuf, String> {
    let input = Path::new(path);
    let name = input
        .file_name()
        .ok_or_else(|| format!("Path has no file or folder name: {path}"))?;
    let parent = input
        .parent()
        .ok_or_else(|| format!("Path has no parent directory: {path}"))?;
    let canonical_parent = dunce::canonicalize(parent)
        .map_err(|e| format!("Failed to resolve parent directory: {e}"))?;
    let resolved = canonical_parent.join(name);
    fs::symlink_metadata(&resolved).map_err(|e| format!("Path not found: {path}: {e}"))?;
    Ok(resolved)
}

fn path_to_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Path is not valid UTF-8".to_string())
}

/// Atomically rename without replacing an entry created by another process.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{renameat_with, RenameFlags, CWD};

    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(io::Error::from)
}

#[cfg(target_os = "windows")]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: Both buffers are NUL-terminated and remain alive for the call.
    let result = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    if fs::symlink_metadata(destination).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    fs::rename(source, destination)
}

/// Cross-filesystem fallback for regular files. `create_new` reserves the destination
/// atomically, so an existing file is never truncated or replaced.
fn copy_then_remove_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    let mut source_file = fs::File::open(source)?;
    let source_permissions = source_file.metadata()?.permissions();
    let mut destination_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;

    let result = (|| -> io::Result<()> {
        io::copy(&mut source_file, &mut destination_file)?;
        destination_file.set_permissions(source_permissions)?;
        destination_file.sync_all()?;
        fs::remove_file(source)?;
        Ok(())
    })();

    if result.is_err() {
        drop(destination_file);
        let _ = fs::remove_file(destination);
    }
    result
}

/// Rename a file or folder beside itself. Existing destinations are never overwritten.
#[tauri::command]
pub fn rename_path(path: String, new_name: String) -> Result<String, String> {
    validate_simple_name(&new_name)?;
    let source = resolve_existing_entry(&path)?;
    let destination = source
        .parent()
        .ok_or_else(|| format!("Path has no parent directory: {path}"))?
        .join(new_name);

    if destination == source {
        return path_to_string(&source);
    }
    if fs::symlink_metadata(&destination).is_ok() {
        let same_entry = dunce::canonicalize(&destination)
            .ok()
            .zip(dunce::canonicalize(&source).ok())
            .is_some_and(|(existing, source)| existing == source);
        if !same_entry {
            return Err(format!(
                "A file or folder already exists: {}",
                destination.display()
            ));
        }
        // Case-only rename on a case-insensitive filesystem targets the same entry.
        fs::rename(&source, &destination).map_err(|e| format!("Failed to rename: {e}"))?;
        return path_to_string(&destination);
    }

    rename_no_replace(&source, &destination)
        .map_err(|e| format!("Failed to rename without overwriting: {e}"))?;
    path_to_string(&destination)
}

/// Move one document into an existing folder. Existing destinations are never overwritten.
#[tauri::command]
pub fn move_document(path: String, destination_dir: String) -> Result<String, String> {
    let source = resolve_existing_entry(&path)?;
    let metadata = fs::symlink_metadata(&source)
        .map_err(|e| format!("Failed to read source metadata: {e}"))?;
    if !metadata.is_file() {
        return Err("Only files can be moved into folders".to_string());
    }

    let destination_dir = dunce::canonicalize(&destination_dir)
        .map_err(|e| format!("Failed to resolve destination folder: {e}"))?;
    if !destination_dir.is_dir() {
        return Err(format!("Not a directory: {}", destination_dir.display()));
    }
    let file_name = source
        .file_name()
        .ok_or_else(|| format!("Source path has no file name: {path}"))?;
    let destination = destination_dir.join(file_name);

    if destination == source {
        return path_to_string(&source);
    }
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "A file already exists in that folder: {}",
            destination.display()
        ));
    }

    match rename_no_replace(&source, &destination) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            copy_then_remove_no_replace(&source, &destination)
                .map_err(|e| format!("Failed to move document across filesystems: {e}"))?;
        }
        Err(error) => return Err(format!("Failed to move document: {error}")),
    }
    path_to_string(&destination)
}

/// Send a file or folder to the operating system trash/recycle bin.
#[tauri::command]
pub fn trash_path(path: String) -> Result<(), String> {
    let entry = resolve_existing_entry(&path)?;
    trash::delete(&entry).map_err(|e| format!("Failed to move item to trash: {e}"))
}

/// Open native print dialog for current webview.
/// Tauri's native implementation is required on macOS, where `window.print()`
/// inside WKWebView does not reliably open print dialog.
#[tauri::command]
pub fn print_webview(webview: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(desktop)]
    {
        return webview
            .print()
            .map_err(|e| format!("Failed to open print dialog: {e}"));
    }

    #[cfg(not(desktop))]
    {
        let _ = webview;
        Err("Printing is not supported on this platform".to_string())
    }
}

// ============================================================================
// App paths
// ============================================================================

fn app_data_dir(app: &AppHandle) -> Result<String, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))?
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "App data dir is not valid UTF-8".to_string())
}

/// Check whether a directory is writable by creating a temp file and removing it.
/// This is more reliable than checking permission bits (some mount scenarios lie).
#[cfg(target_os = "windows")]
fn is_dir_writable(dir: &std::path::Path) -> bool {
    let test_file = dir.join(".zcode_write_test");
    match std::fs::File::create(&test_file) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
    }
}

/// Default data directory for pin/output folders.
///
/// Windows: prefers the exe's parent directory (portable mode).
/// Falls back to `app_data_dir()` if the exe dir is not writable
/// (e.g. installed under Program Files).
#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_default_data_dir(app: AppHandle) -> Result<String, String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if is_dir_writable(parent) {
                return parent
                    .to_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| "Exe parent dir is not valid UTF-8".to_string());
            }
        }
    }
    app_data_dir(&app)
}

/// Default data directory for pin/output folders.
///
/// macOS / Linux: always use the system-standard app data directory.
/// Portable mode is not a concept on these platforms.
#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_default_data_dir(app: AppHandle) -> Result<String, String> {
    app_data_dir(&app)
}

/// Join two path components with the platform-native separator.
#[tauri::command]
pub fn join_path(base: String, child: String) -> Result<String, String> {
    let joined = std::path::Path::new(&base).join(&child);
    joined
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Joined path is not valid UTF-8".to_string())
}

/// List all files (not directories) in a single directory.
/// Non-recursive, no extension filter — returns every visible file.
#[tauri::command]
pub fn list_folder_flat(folder: String) -> Result<Vec<DirNode>, String> {
    let dir = Path::new(&folder);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", folder));
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {e}"))?;
    let mut files: Vec<DirNode> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(fname) = path.file_name() {
            if fname.to_string_lossy().starts_with('.') {
                continue;
            }
        }
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
        files.push(DirNode {
            name: fname,
            path: path.to_string_lossy().to_string(),
            is_dir: false,
            modified,
            children: None,
        });
    }
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(files)
}

/// Copy a single file into a destination folder. Never overwrites — if a
/// same-name file already exists the copy is renamed with a (1), (2), … suffix.
#[tauri::command]
pub fn copy_file_to_folder(source_path: String, dest_folder: String) -> Result<String, String> {
    use std::fs;
    use std::path::Path;

    let source = Path::new(&source_path);
    let dest_dir = Path::new(&dest_folder);

    fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to prepare destination: {e}"))?;

    let file_name = source
        .file_name()
        .ok_or_else(|| "Source path has no file name".to_string())?;

    let mut dest_path = dest_dir.join(file_name);
    if dest_path.exists() {
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = source.extension().and_then(|s| s.to_str());
        let mut n = 1;
        const MAX_ATTEMPTS: u32 = 10_000;
        loop {
            if n > MAX_ATTEMPTS {
                return Err(format!(
                    "Too many name collisions in destination folder (tried {MAX_ATTEMPTS} names)"
                ));
            }
            let candidate_name = match ext {
                Some(e) => format!("{stem} ({n}).{e}"),
                None => format!("{stem} ({n})"),
            };
            let candidate = dest_dir.join(candidate_name);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(_) => {
                    dest_path = candidate;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(format!("Failed to create destination file: {e}"));
                }
            }
            n += 1;
        }
    }

    if let Err(e) = fs::copy(source, &dest_path) {
        let _ = std::fs::remove_file(&dest_path);
        return Err(format!("Failed to copy file: {e}"));
    }
    dest_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Destination path is not valid UTF-8".to_string())
}

/// Open a file or folder path in the system file manager.
#[tauri::command]
pub fn open_in_shell(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    // Create the directory if it doesn't exist (for default output/pin folders).
    // Propagate the error so the frontend can surface it to the user — the
    // default-path logic already avoids unwritable exe dirs, so a failure here
    // means something unusual (e.g. the user pointed at a removed external drive).
    if !p.exists() {
        if let Err(e) = std::fs::create_dir_all(p) {
            return Err(format!("Failed to create directory: {e}"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_rename_path_renames_file_without_overwriting() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().join("old.md");
        std::fs::write(&original, "content").unwrap();

        let renamed =
            rename_path(original.to_str().unwrap().to_string(), "new.md".to_string()).unwrap();

        assert!(!original.exists());
        assert_eq!(std::fs::read_to_string(renamed).unwrap(), "content");

        std::fs::write(tmp.path().join("taken.md"), "taken").unwrap();
        let collision = rename_path(
            tmp.path().join("new.md").to_str().unwrap().to_string(),
            "taken.md".to_string(),
        );
        assert!(collision.is_err());
    }

    #[test]
    fn test_rename_no_replace_preserves_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.md");
        let destination = tmp.path().join("destination.md");
        std::fs::write(&source, "source").unwrap();
        std::fs::write(&destination, "destination").unwrap();

        let result = rename_no_replace(&source, &destination);

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "source");
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "destination");
    }

    #[test]
    fn test_copy_then_remove_no_replace_moves_file_safely() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.md");
        let destination = tmp.path().join("destination.md");
        std::fs::write(&source, "source").unwrap();

        copy_then_remove_no_replace(&source, &destination).unwrap();

        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "source");
    }

    #[test]
    fn test_rename_path_rejects_directory_components() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().join("old.md");
        std::fs::write(&original, "content").unwrap();

        let result = rename_path(
            original.to_str().unwrap().to_string(),
            "sub/new.md".to_string(),
        );

        assert!(result.is_err());
        assert!(original.exists());
    }

    #[test]
    fn test_move_document_moves_file_and_rejects_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("note.md");
        let destination_dir = tmp.path().join("folder");
        std::fs::write(&source, "note").unwrap();
        std::fs::create_dir(&destination_dir).unwrap();

        let moved = move_document(
            source.to_str().unwrap().to_string(),
            destination_dir.to_str().unwrap().to_string(),
        )
        .unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "note");

        let second = tmp.path().join("note.md");
        std::fs::write(&second, "second").unwrap();
        let collision = move_document(
            second.to_str().unwrap().to_string(),
            destination_dir.to_str().unwrap().to_string(),
        );
        assert!(collision.is_err());
        assert_eq!(std::fs::read_to_string(second).unwrap(), "second");
    }

    /// Test copy_file_to_folder: basic copy into empty dir
    #[test]
    fn test_copy_file_to_folder_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("source.txt");
        std::fs::write(&src, "hello world").unwrap();

        let dest_dir = tmp.path().join("dest");
        std::fs::create_dir(&dest_dir).unwrap();

        let result = copy_file_to_folder(
            src.to_str().unwrap().to_string(),
            dest_dir.to_str().unwrap().to_string(),
        );
        assert!(
            result.is_ok(),
            "copy_file_to_folder failed: {:?}",
            result.err()
        );
        let dest_path = result.unwrap();

        assert!(Path::new(&dest_path).exists());
        let content = std::fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, "hello world");
    }

    /// Test copy_file_to_folder: never overwrite — rename with suffix
    #[test]
    fn test_copy_file_to_folder_no_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("doc.pdf");
        std::fs::write(&src, "pdf content").unwrap();

        let dest_dir = tmp.path().join("sources");
        std::fs::create_dir(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("doc.pdf"), "pre-existing").unwrap();

        let result = copy_file_to_folder(
            src.to_str().unwrap().to_string(),
            dest_dir.to_str().unwrap().to_string(),
        );
        assert!(result.is_ok());
        let dest_path = result.unwrap();

        assert_ne!(
            dest_path,
            dest_dir.join("doc.pdf").to_str().unwrap().to_string()
        );
        assert!(dest_path.contains("doc ("));
        let content = std::fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, "pdf content");

        let pre_existing = std::fs::read_to_string(dest_dir.join("doc.pdf")).unwrap();
        assert_eq!(pre_existing, "pre-existing");
    }

    /// Test copy_file_to_folder: creates dest dir if needed
    #[test]
    fn test_copy_file_to_folder_creates_dest() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("data.csv");
        std::fs::write(&src, "a,b,c").unwrap();

        let dest_dir = tmp.path().join("nonexistent").join("sub");

        let result = copy_file_to_folder(
            src.to_str().unwrap().to_string(),
            dest_dir.to_str().unwrap().to_string(),
        );
        assert!(
            result.is_ok(),
            "copy_file_to_folder failed: {:?}",
            result.err()
        );
        let dest_path = result.unwrap();
        assert!(Path::new(&dest_path).exists());
        let content = std::fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, "a,b,c");
    }

    /// Test list_folder_flat: lists files, not dirs, sorted case-insensitively
    #[test]
    fn test_list_folder_flat_basic() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();
        std::fs::write(tmp.path().join(".hidden"), "hidden").unwrap();

        let result = list_folder_flat(tmp.path().to_str().unwrap().to_string());
        assert!(result.is_ok());
        let files = result.unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "a.txt");
        assert_eq!(files[1].name, "b.txt");
        assert!(files[0].path.ends_with("a.txt"));
        assert!(files[1].path.ends_with("b.txt"));
        assert!(!files[0].is_dir);
        assert!(!files[1].is_dir);
    }

    /// Test list_folder_flat: empty directory
    #[test]
    fn test_list_folder_flat_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let result = list_folder_flat(tmp.path().to_str().unwrap().to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    /// Test list_folder_flat: not a directory
    #[test]
    fn test_list_folder_flat_not_a_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("notadir.txt");
        std::fs::write(&file, "content").unwrap();

        let result = list_folder_flat(file.to_str().unwrap().to_string());
        assert!(result.is_err());
    }

    /// Test copy_file_to_folder: source doesn't exist
    #[test]
    fn test_copy_file_to_folder_source_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dest_dir = tmp.path().join("dest");
        std::fs::create_dir(&dest_dir).unwrap();

        let result = copy_file_to_folder(
            "/nonexistent/file.txt".to_string(),
            dest_dir.to_str().unwrap().to_string(),
        );
        assert!(result.is_err());
    }

    /// Test copy_file_to_folder: incremental counter on many collisions
    #[test]
    fn test_copy_file_to_folder_many_collisions() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("report.xlsx");
        std::fs::write(&src, "data").unwrap();

        let dest_dir = tmp.path().join("sources");
        std::fs::create_dir(&dest_dir).unwrap();

        std::fs::write(dest_dir.join("report.xlsx"), "v0").unwrap();
        std::fs::write(dest_dir.join("report (1).xlsx"), "v1").unwrap();
        std::fs::write(dest_dir.join("report (2).xlsx"), "v2").unwrap();

        let result = copy_file_to_folder(
            src.to_str().unwrap().to_string(),
            dest_dir.to_str().unwrap().to_string(),
        );
        assert!(result.is_ok());
        let dest_path = result.unwrap();
        assert!(dest_path.contains("report (3)"));
    }
}
