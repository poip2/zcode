import { writable } from "svelte/store";

export interface DocumentState {
  filePath: string | null;
  fileName: string | null;
  content: string;
  renderedHtml: string;
  frontmatter: Record<string, unknown> | null;
  wordCount: number;
  loading: boolean;
  error: string | null;
}

const initial: DocumentState = {
  filePath: null,
  fileName: null,
  content: "",
  renderedHtml: "",
  frontmatter: null,
  wordCount: 0,
  loading: false,
  error: null,
};

export const document = writable<DocumentState>(initial);
