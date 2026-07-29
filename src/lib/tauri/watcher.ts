import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { reloadCurrentFile } from "./files";

let unlisten: UnlistenFn | null = null;
let reloadTimeout: ReturnType<typeof setTimeout> | null = null;
let activeFilePath: string | null = null;
let watcherRevision = 0;
let watcherQueue: Promise<void> = Promise.resolve();

// Track last save time per file path to suppress self-triggered reloads
const lastSavedAt = new Map<string, number>();
const OWN_SAVE_SUPPRESSION_MS = 1500;

function normalizePath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  return navigator.platform.includes("Win") ? normalized.toLowerCase() : normalized;
}

function clearFrontendWatcher(): void {
  if (reloadTimeout) {
    clearTimeout(reloadTimeout);
    reloadTimeout = null;
  }
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  activeFilePath = null;
}

function enqueueWatcherOperation(operation: () => Promise<void>): Promise<void> {
  const result = watcherQueue.then(operation, operation);
  // Keep later operations runnable after a failed invoke/listen call.
  watcherQueue = result.catch(() => {});
  return result;
}

/** Start watching the given file. Operations are serialized so stale starts cannot win. */
export function startFileWatcher(filePath: string): Promise<void> {
  const revision = ++watcherRevision;

  return enqueueWatcherOperation(async () => {
    if (revision !== watcherRevision) return;
    clearFrontendWatcher();
    activeFilePath = filePath;

    try {
      const nextUnlisten = await listen<{ path: string }>("file-changed", (event) => {
        const activePath = activeFilePath;
        if (!activePath || normalizePath(event.payload.path) !== normalizePath(activePath)) return;

        if (reloadTimeout) clearTimeout(reloadTimeout);
        reloadTimeout = setTimeout(() => {
          if (!activeFilePath || normalizePath(activeFilePath) !== normalizePath(activePath)) return;
          const savedAt = lastSavedAt.get(activePath);
          if (savedAt && Date.now() - savedAt < OWN_SAVE_SUPPRESSION_MS) return;
          reloadCurrentFile(activePath);
        }, 100);
      });

      if (revision !== watcherRevision) {
        nextUnlisten();
        activeFilePath = null;
        return;
      }
      unlisten = nextUnlisten;
      await invoke("start_watching", { path: filePath });
    } catch (error) {
      if (revision === watcherRevision) clearFrontendWatcher();
      throw error;
    }
  });
}

/** Stop watching. Invalidates and queues behind any in-flight start operation. */
export function stopFileWatcher(): void {
  ++watcherRevision;
  clearFrontendWatcher();
  void enqueueWatcherOperation(async () => {
    await invoke("stop_watching");
  }).catch(() => {});
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
