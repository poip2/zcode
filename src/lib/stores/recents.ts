import { writable, get } from "svelte/store";
import { getStore } from "./sharedStore";

export interface RecentEntry {
  path: string;
  name: string;
  openedAt: number;
}

const STORE_KEY = "recents";
const MAX_RECENTS = 20;

const recentList = writable<RecentEntry[]>([]);

// Load persisted recents on first access
let loaded = false;
async function ensureLoaded() {
  if (loaded) return;
  loaded = true;
  try {
    const store = await getStore();
    const saved = await store.get<RecentEntry[]>(STORE_KEY);
    if (saved && Array.isArray(saved)) {
      recentList.set(saved);
    }
  } catch {
    // Ignore load errors — start empty
  }
}

async function persist(list: RecentEntry[]) {
  try {
    const store = await getStore();
    await store.set(STORE_KEY, list);
    await store.save();
  } catch {
    // Ignore persist errors
  }
}

function addRecent(path: string) {
  const name = path.split(/[/\\]/).pop() ?? path;
  const list = get(recentList);
  const filtered = list.filter((e) => e.path !== path);
  const updated: RecentEntry[] = [
    { path, name, openedAt: Date.now() },
    ...filtered,
  ].slice(0, MAX_RECENTS);
  recentList.set(updated);
  persist(updated);
}

export const recents = {
  subscribe: recentList.subscribe,

  addRecent,

  /** Call once at app startup to hydrate from disk */
  load: ensureLoaded,

  /** Clear all recents */
  clear: async () => {
    recentList.set([]);
    await persist([]);
  },
};
