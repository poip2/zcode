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
    },
  };
}

export const folderTree = createFolderTreeStore();
