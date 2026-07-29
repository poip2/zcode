import { writable, get } from "svelte/store";
import { listDirTree } from "$lib/tauri/files";

export interface DirNode {
  name: string;
  path: string;
  is_dir: boolean;
  modified?: number | null;
  children?: DirNode[] | null;
}

export interface FolderTreeState {
  rootPath: string | null;
  tree: DirNode | null;
  loading: boolean;
  error: string | null;
}

function createFolderTreeStore() {
  const state = writable<FolderTreeState>({
    rootPath: null,
    tree: null,
    loading: false,
    error: null,
  });

  const expandedPaths = writable<Set<string>>(new Set());

  return {
    subscribe: state.subscribe,

    setRoot(p: string | null) {
      state.update((s) => ({ ...s, rootPath: p, tree: null, error: null }));
    },

    setTree(tree: DirNode | null) {
      state.update((s) => ({ ...s, tree, loading: false, error: null }));
    },

    setLoading(loading: boolean) {
      state.update((s) => ({ ...s, loading, error: loading ? null : s.error }));
    },

    setError(error: string | null) {
      state.update((s) => ({ ...s, error, loading: false }));
    },

    async refresh() {
      const current = get(state);
      if (!current.rootPath) return;
      try {
        const tree = await listDirTree(current.rootPath);
        state.update((s) => ({ ...s, tree, loading: false, error: null }));
      } catch (err) {
        state.update((s) => ({ ...s, error: `Failed to read folder: ${err}`, loading: false }));
      }
    },

    // Expanded paths
    expanded: {
      subscribe: expandedPaths.subscribe,
      toggle(path: string) {
        expandedPaths.update((set) => {
          const next = new Set(set);
          if (next.has(path)) {
            next.delete(path);
          } else {
            next.add(path);
          }
          return next;
        });
      },
      isExpanded(path: string): boolean {
        return get(expandedPaths).has(path);
      },
      ensure(path: string) {
        expandedPaths.update((set) => {
          if (set.has(path)) return set;
          const next = new Set(set);
          next.add(path);
          return next;
        });
      },
      removeTree(path: string) {
        const base = path.replace(/\\/g, "/").replace(/\/$/, "");
        expandedPaths.update((set) => {
          const next = new Set(
            [...set].filter((item) => {
              const normalized = item.replace(/\\/g, "/");
              return normalized !== base && !normalized.startsWith(`${base}/`);
            }),
          );
          return next;
        });
      },
      replaceTree(oldPath: string, newPath: string) {
        const oldBase = oldPath.replace(/\\/g, "/").replace(/\/$/, "");
        const separator = newPath.includes("\\") ? "\\" : "/";
        expandedPaths.update((set) => {
          const next = new Set<string>();
          for (const item of set) {
            const normalized = item.replace(/\\/g, "/");
            if (normalized === oldBase || normalized.startsWith(`${oldBase}/`)) {
              const suffix = normalized.slice(oldBase.length).replaceAll("/", separator);
              next.add(`${newPath}${suffix}`);
            } else {
              next.add(item);
            }
          }
          return next;
        });
      },
    },
  };
}

export const folderTree = createFolderTreeStore();
