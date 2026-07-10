import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
