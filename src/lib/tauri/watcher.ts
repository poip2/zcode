import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { reloadCurrentFile } from "./files";

let unlisten: UnlistenFn | null = null;
let reloadTimeout: ReturnType<typeof setTimeout> | null = null;

// Track last save time per file path to suppress self-triggered reloads
const lastSavedAt = new Map<string, number>();
const OWN_SAVE_SUPPRESSION_MS = 1500;

/** Start watching the given file for external changes. */
export async function startFileWatcher(filePath: string): Promise<void> {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  if (reloadTimeout) {
    clearTimeout(reloadTimeout);
    reloadTimeout = null;
  }

  unlisten = await listen<{ path: string }>("file-changed", () => {
    // Debounce on frontend too — editors may trigger multiple events
    if (reloadTimeout) clearTimeout(reloadTimeout);
    reloadTimeout = setTimeout(() => {
      // Skip reload if this file-changed event was triggered by our own save
      const savedAt = lastSavedAt.get(filePath);
      if (savedAt && Date.now() - savedAt < OWN_SAVE_SUPPRESSION_MS) {
        return;
      }
      reloadCurrentFile(filePath);
    }, 100);
  });
}

/** Stop the file watcher. */
export function stopFileWatcher(): void {
  if (reloadTimeout) {
    clearTimeout(reloadTimeout);
    reloadTimeout = null;
  }
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}

/**
 * Mark that a file was just saved by this application.
 * Prevents the watcher from immediately re-loading our own writes.
 */
export function markSaved(filePath: string): void {
  const now = Date.now();
  lastSavedAt.set(filePath, now);
  for (const [key, t] of lastSavedAt) {
    if (now - t > OWN_SAVE_SUPPRESSION_MS) {
      lastSavedAt.delete(key);
    }
  }
}

// ============================================================================
// Skills watcher
// ============================================================================

let skillsUnlisten: UnlistenFn | null = null;

/** Start the backend skills watcher and listen for skills-changed events. */
export async function startSkillsWatcher(cwd: string): Promise<void> {
  // Start the Rust-side watcher
  await invoke("start_skills_watching", { cwd });
}

/** Stop the backend skills watcher. */
export async function stopSkillsWatcher(): Promise<void> {
  await invoke("stop_skills_watching");
}

/**
 * Listen for skills-changed events globally.
 * Calls the provided callback each time a skill file changes on disk.
 */
export async function listenSkillsChanged(onChange: () => void): Promise<UnlistenFn> {
  if (skillsUnlisten) {
    skillsUnlisten();
  }
  skillsUnlisten = await listen("skills-changed", () => {
    onChange();
  });
  return skillsUnlisten;
}

/** Stop listening for skills changes. */
export function unlistenSkillsChanged(): void {
  if (skillsUnlisten) {
    skillsUnlisten();
    skillsUnlisten = null;
  }
}
