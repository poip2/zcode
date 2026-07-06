import { writable } from "svelte/store";
import { Store } from "@tauri-apps/plugin-store";

const STORE_KEY = "pinnedFolder";

let storePromise: Promise<Store> | null = null;

function getStore(): Promise<Store> {
  if (!storePromise) {
    storePromise = Store.load("zcode-recents.json");
  }
  return storePromise;
}

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
    }
  } catch {
    // Ignore load errors — start with no pin
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
