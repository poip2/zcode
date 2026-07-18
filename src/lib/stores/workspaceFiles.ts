import { writable } from "svelte/store";
import { listFolderFlat } from "$lib/tauri/files";
import type { DirNode } from "$lib/stores/folderTree";

export const sourcesFiles = writable<DirNode[]>([]);
export const outputFiles = writable<DirNode[]>([]);

async function loadFlatFiles(path: string): Promise<DirNode[]> {
  if (!path) return [];
  try {
    return await listFolderFlat(path);
  } catch {
    return [];
  }
}

export async function reloadSourcesFiles(path: string) {
  sourcesFiles.set(await loadFlatFiles(path));
}

export async function reloadOutputFiles(path: string) {
  outputFiles.set(await loadFlatFiles(path));
}
