use std::fs;
use std::path::Path;
use serde::Serialize;
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
    absolute
        .canonicalize()
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
    if depth > MAX_TREE_DEPTH {
        return None;
    }

    let entries = fs::read_dir(dir).ok()?;
    let mut children: Vec<DirNode> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories
        if let Some(name) = path.file_name() {
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
        }

        if path.is_dir() {
            // Skip common non-content directories
            if let Some(name) = path.file_name() {
                let n = name.to_string_lossy();
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
                // Only include directory if it has children
                if subnode.children.as_ref().map_or(false, |c| !c.is_empty()) {
                    children.push(subnode);
                }
            }
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" || ext == "markdown" || ext == "mdown" || ext == "mkd" {
                    let name = path
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
                        name,
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
            if a.is_dir { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    if children.is_empty() && depth > 0 {
        return None;
    }

    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let children = if children.is_empty() { None } else { Some(children) };

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

    let filename = if name.ends_with(".md") || name.ends_with(".markdown") || name.ends_with(".mdown") || name.ends_with(".mkd") {
        name
    } else {
        format!("{}.md", name)
    };

    validate_simple_name(&filename)?;
    let canonical_dir = dir_path.canonicalize()
        .map_err(|e| format!("Failed to resolve directory: {}", e))?;
    let file_path = canonical_dir.join(&filename);

    if file_path.exists() {
        return Err(format!("File already exists: {}", file_path.display()));
    }

    fs::write(&file_path, "").map_err(|e| format!("Failed to create file: {}", e))?;

    file_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Path is not valid UTF-8"))
}

#[tauri::command]
pub fn create_folder(dir: String, name: String) -> Result<String, String> {
    let dir_path = Path::new(&dir);
    if !dir_path.is_dir() {
        return Err(format!("Not a directory: {}", dir));
    }

    validate_simple_name(&name)?;
    let canonical_dir = dir_path.canonicalize()
        .map_err(|e| format!("Failed to resolve directory: {}", e))?;
    let folder_path = canonical_dir.join(&name);

    if folder_path.exists() {
        return Err(format!("Folder already exists: {}", folder_path.display()));
    }

    fs::create_dir(&folder_path).map_err(|e| format!("Failed to create folder: {}", e))?;

    folder_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Path is not valid UTF-8"))
}

fn validate_simple_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let p = Path::new(name);
    if p.file_name().map_or(true, |f| f != p.as_os_str()) {
        return Err("Name must be a simple file or folder name without directory components".to_string());
    }
    if name == "." || name == ".." {
        return Err("Invalid name".to_string());
    }
    Ok(())
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
