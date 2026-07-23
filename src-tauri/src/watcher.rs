//! File system watcher — detects external changes to the currently-open file.
//! Adapted from mdhero (src-tauri/src/watcher.rs).

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub struct WatcherState {
    watcher: Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>>,
    watched_path: Mutex<Option<String>>,
}

impl Default for WatcherState {
    fn default() -> Self {
        Self {
            watcher: Mutex::new(None),
            watched_path: Mutex::new(None),
        }
    }
}

#[tauri::command]
pub fn start_watching(app: AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<WatcherState>();
    let mut watcher_lock = state.watcher.lock().map_err(|e| e.to_string())?;
    let mut path_lock = state.watched_path.lock().map_err(|e| e.to_string())?;

    // Stop existing watcher
    *watcher_lock = None;

    let watch_path = PathBuf::from(&path);
    let target_file = watch_path.clone();

    // Watch the parent directory to survive atomic saves (delete + rename)
    let watch_dir = watch_path
        .parent()
        .ok_or_else(|| "Cannot determine parent directory".to_string())?
        .to_path_buf();

    let app_handle = app.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            match res {
                Ok(events) => {
                    for event in events {
                        if event.kind == DebouncedEventKind::Any {
                            // Only emit if the changed file matches our target
                            if event.path == target_file {
                                let _ = app_handle.emit(
                                    "file-changed",
                                    serde_json::json!({
                                        "path": event.path.to_string_lossy()
                                    }),
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Watch error: {:?}", e);
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    debouncer
        .watcher()
        .watch(&watch_dir, notify::RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch directory: {}", e))?;

    *watcher_lock = Some(debouncer);
    *path_lock = Some(path);

    Ok(())
}

#[tauri::command]
pub fn stop_watching(app: AppHandle) -> Result<(), String> {
    let state = app.state::<WatcherState>();
    let mut watcher_lock = state.watcher.lock().map_err(|e| e.to_string())?;
    let mut path_lock = state.watched_path.lock().map_err(|e| e.to_string())?;
    *watcher_lock = None;
    *path_lock = None;
    Ok(())
}

// ============================================================================
// Skills watcher — monitors .zcode/skills/ and ~/.config/zcode/skills/
// ============================================================================

pub struct SkillWatcherState {
    watcher: Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>>,
}

fn is_skill_change_path(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "SKILL.md")
        || path
            .parent()
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "skills"))
}

impl Default for SkillWatcherState {
    fn default() -> Self {
        Self {
            watcher: Mutex::new(None),
        }
    }
}

/// Start watching skill directories for changes.
///
/// Watches project and user directories recursively:
/// - `<cwd>/.zcode/skills` (project-level)
/// - `<user_config>/zcode/skills` (platform user-level)
/// - `~/.config/zcode/skills` (documented user-level, when different)
///
/// Creates directories if they don't exist so the watcher can catch the first
/// creation event. Stops and replaces any previously running skills watcher.
#[tauri::command]
pub fn start_skills_watching(app: AppHandle, cwd: String) -> Result<(), String> {
    let state = app.state::<SkillWatcherState>();
    let mut watcher_lock = state.watcher.lock().map_err(|e| e.to_string())?;

    // Stop existing watcher first (drop = unwatch + stop)
    *watcher_lock = None;

    let user_config_dir = dirs::config_dir()
        .ok_or_else(|| "No config directory found".to_string())?
        .join("zcode");
    let user_dirs = crate::skills::user_skill_roots(Some(&user_config_dir));
    let project_dir = PathBuf::from(&cwd).join(".zcode").join("skills");

    // Ensure directories exist so the watcher can be registered even when empty.
    // This also handles the case where no skill directory has been created yet.
    for dir in user_dirs.iter().chain(std::iter::once(&project_dir)) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    }

    let app_handle = app.clone();
    let last_emit = Cell::new(Instant::now() - Duration::from_secs(10));

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            match res {
                Ok(events) => {
                    // Check names, not path.is_file(): deleted SKILL.md paths no
                    // longer exist and must still trigger a frontend refresh.
                    let has_skill_change = events
                        .iter()
                        .any(|event| is_skill_change_path(&event.path));
                    if has_skill_change {
                        // Rate-limit: at most one emit per 2 seconds.
                        // Prevents feedback loops where list_skills reads SKILL.md,
                        // updates atime, triggers another inotify event, and repeats.
                        let now = Instant::now();
                        if now.duration_since(last_emit.get()) >= Duration::from_secs(2) {
                            last_emit.set(now);
                            eprintln!("[zcode] skills-watcher: SKILL.md change detected, emitting skills-changed");
                            let _ = app_handle.emit("skills-changed", ());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[zcode] skills-watcher: watch error: {e:?}");
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create skills watcher: {}", e))?;

    for dir in user_dirs.iter().chain(std::iter::once(&project_dir)) {
        debouncer
            .watcher()
            .watch(dir, notify::RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch {}: {}", dir.display(), e))?;
    }

    *watcher_lock = Some(debouncer);

    eprintln!(
        "[zcode] skills-watcher: started watching user={:?}, project={}",
        user_dirs,
        project_dir.display()
    );
    Ok(())
}

/// Stop the skills watcher.
#[tauri::command]
pub fn stop_skills_watching(app: AppHandle) -> Result<(), String> {
    let state = app.state::<SkillWatcherState>();
    let mut watcher_lock = state.watcher.lock().map_err(|e| e.to_string())?;
    *watcher_lock = None;
    eprintln!("[zcode] skills-watcher: stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deleted_skill_file_is_detected() {
        let deleted = PathBuf::from("/tmp/project/.zcode/skills/demo/SKILL.md");
        assert!(is_skill_change_path(&deleted));
        assert!(!is_skill_change_path(&deleted.with_file_name("README.md")));
    }

    #[test]
    fn test_create_skill_dirs_and_watch() {
        let tmp = tempfile::tempdir().unwrap();
        let project_root = tmp.path().join("myproject");
        std::fs::create_dir_all(&project_root).unwrap();

        let user_config = tmp.path().join("config").join("zcode");
        let user_skills = user_config.join("skills");
        let project_skills = project_root.join(".zcode").join("skills");

        // Directories should not exist yet
        assert!(!user_skills.exists());
        assert!(!project_skills.exists());

        // create_dir_all should make them
        std::fs::create_dir_all(&user_skills).unwrap();
        std::fs::create_dir_all(&project_skills).unwrap();
        assert!(user_skills.exists());
        assert!(project_skills.exists());

        // After creation, watch should succeed
        let mut debouncer = new_debouncer(
            Duration::from_millis(100),
            move |_res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {},
        )
        .unwrap();

        debouncer
            .watcher()
            .watch(&user_skills, notify::RecursiveMode::Recursive)
            .unwrap();
        debouncer
            .watcher()
            .watch(&project_skills, notify::RecursiveMode::Recursive)
            .unwrap();

        // Creating a SKILL.md inside should trigger the watcher (we just verify it doesn't crash)
        std::fs::create_dir_all(project_skills.join("my-skill")).unwrap();
        std::fs::write(
            project_skills.join("my-skill").join("SKILL.md"),
            "---\nname: my-skill\ndescription: test\n---\n# Test\n",
        )
        .unwrap();

        // Give the debouncer a moment to fire
        std::thread::sleep(Duration::from_millis(200));

        eprintln!("✅ Skill dir creation + watch + file creation succeeded");
    }
}
