import { writable } from "svelte/store";

export interface ExternalFileState {
  path: string;
  name: string;
}

export const externalFile = writable<ExternalFileState | null>(null);
