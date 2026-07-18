import { writable } from "svelte/store";
import { getStore } from "./sharedStore";
import { load as loadSettings } from "./settings";
import { getDefaultDataDir, joinPath } from "$lib/tauri/files";

const STORE_KEY = "pinnedFolder";

const pinnedPath = writable<string | null>(null);

let loaded = false;

async function ensureLoaded() {
  if (loaded) return;
  loaded = true;
  try {
    const store = await getStore();
    const saved = await store.get<string>(STORE_KEY);
    if (saved) {
      pinnedPath.set(saved);
      return;
    }
  } catch {
    // Ignore load errors
  }

  // Fallback: use settings.pinFolder if set
  try {
    const settings = await loadSettings();
    if (settings.pinFolder) {
      pinnedPath.set(settings.pinFolder);
      return;
    }
  } catch {
    // Ignore settings load errors
  }

  // Last resort: compute default pin path from dataDir
  try {
    const dataDir = await getDefaultDataDir();
    const defaultPin = await joinPath(dataDir, "pin");
    pinnedPath.set(defaultPin);
  } catch {
    // Ignore path resolution errors — start with no pin
  }
}

async function persist(path: string | null) {
  try {
    const store = await getStore();
    if (path) {
      await store.set(STORE_KEY, path);
    } else {
      await store.delete(STORE_KEY);
    }
    await store.save();
  } catch {
    // Ignore persist errors
  }
}

export const pinnedFolder = {
  subscribe: pinnedPath.subscribe,

  /** Call once at startup to hydrate from disk */
  load: ensureLoaded,

  /** Pin a folder path */
  async pin(path: string) {
    pinnedPath.set(path);
    await persist(path);
  },

  /** Remove the pin */
  async unpin() {
    pinnedPath.set(null);
    await persist(null);
  },
};
