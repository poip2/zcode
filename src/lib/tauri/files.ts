import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { get } from "svelte/store";
import { document } from "../stores/document";
import { type DirNode } from "../stores/folderTree";
import { renderFull } from "../renderer/pipeline";
import { markSaved } from "./watcher";

export async function readMarkdownFile(path: string): Promise<string> {
  return invoke<string>("read_markdown_file", { path });
}

export async function saveFile(path: string, content: string): Promise<void> {
  await invoke("write_markdown_file", { path, content });
}

export async function resolvePath(path: string): Promise<string> {
  return invoke<string>("resolve_path", { path });
}

export function getBaseDir(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const idx = normalized.lastIndexOf("/");
  return idx >= 0 ? normalized.slice(0, idx) : ".";
}

export async function loadFile(path: string): Promise<void> {
  const absolutePath = await resolvePath(path);
  const fileName = absolutePath.replace(/\\/g, "/").split("/").pop() ?? absolutePath;
  const baseDir = getBaseDir(absolutePath);

  document.set({
    filePath: absolutePath,
    fileName,
    content: "",
    renderedHtml: "",
    frontmatter: null,
    wordCount: 0,
    loading: true,
    error: null,
  });

  try {
    const content = await readMarkdownFile(absolutePath);
    const result = renderFull(content, baseDir);
    await allowAssets(result.assetPaths);

    document.set({
      filePath: absolutePath,
      fileName,
      content,
      renderedHtml: result.html,
      frontmatter: result.frontmatter,
      wordCount: result.wordCount,
      loading: false,
      error: null,
    });

    getCurrentWindow().setTitle(`${fileName} — zcode`).catch(() => {});
    invoke("start_watching", { path: absolutePath }).catch(() => {});
  } catch (err) {
    document.set({
      filePath: absolutePath,
      fileName,
      content: "",
      renderedHtml: "",
      frontmatter: null,
      wordCount: 0,
      loading: false,
      error: `Failed to open file: ${err}`,
    });
  }
}

export async function openFileDialog(): Promise<string | null> {
  try {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Markdown",
          extensions: ["md", "markdown", "mdown", "mkd", "txt"],
        },
      ],
    });
    if (selected) {
      const path = typeof selected === "string" ? selected : (selected as any)?.path ?? String(selected);
      return path;
    }
  } catch (err) {
    console.error("File dialog error:", err);
  }
  return null;
}

export async function reloadCurrentFile(path: string, isOwnSave = false): Promise<void> {
  try {
    const absolutePath = await resolvePath(path);
    const content = await readMarkdownFile(absolutePath);

    // Skip update if the content hasn't actually changed.
    // This prevents unnecessary DOM destruction from {@html} in MarkdownRenderer,
    // which would cause the preview to visibly flash/close on every reload.
    const current = get(document);
    if (current.content === content && current.filePath === absolutePath) {
      return;
    }

    const baseDir = getBaseDir(absolutePath);
    const result = renderFull(content, baseDir);
    const fileName = absolutePath.replace(/\\/g, "/").split("/").pop() ?? absolutePath;

    await allowAssets(result.assetPaths);

    if (isOwnSave) {
      markSaved(absolutePath);
    }

    document.set({
      filePath: absolutePath,
      fileName,
      content,
      renderedHtml: result.html,
      frontmatter: result.frontmatter,
      wordCount: result.wordCount,
      loading: false,
      error: null,
    });
  } catch (err) {
    console.error("Failed to reload file:", err);
  }
}

export async function allowAssets(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  await invoke("allow_assets", { paths }).catch(() => {});
}

// ---- Folder tree commands ----

export async function listDirTree(rootPath: string): Promise<DirNode> {
  return invoke<DirNode>("read_dir_tree", { root: rootPath });
}

export async function listFolderFlat(folder: string): Promise<DirNode[]> {
  return invoke<DirNode[]>("list_folder_flat", { folder });
}

export async function createMarkdownFile(dir: string, name: string): Promise<string> {
  return invoke<string>("create_markdown_file", { dir, name });
}

export async function createFolder(dir: string, name: string): Promise<string> {
  return invoke<string>("create_folder", { dir, name });
}

export async function pathExists(path: string): Promise<boolean> {
  return invoke<boolean>("path_exists", { path });
}

/** Get the default data directory for pin/output folders. Platform-specific logic in Rust. */
export async function getDefaultDataDir(): Promise<string> {
  return invoke<string>("get_default_data_dir");
}

/** Join two path components with the platform-native separator. */
export async function joinPath(base: string, child: string): Promise<string> {
  return invoke<string>("join_path", { base, child });
}

/**
 * Copy a single file into a destination folder. Never overwrites — if a
 * same-name file already exists the copy is renamed with a (1), (2), … suffix.
 * Returns the absolute path of the newly created copy.
 */
export async function copyFileToFolder(sourcePath: string, destFolder: string): Promise<string> {
  return invoke<string>("copy_file_to_folder", { sourcePath, destFolder });
}

/** Open a path in the system file manager (creates the directory if needed). */
export async function openInShell(path: string): Promise<void> {
  return invoke("open_in_shell", { path });
}

export async function openFolderDialog(): Promise<string | null> {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
    });
    if (selected) {
      const path = typeof selected === "string" ? selected : (selected as any)?.path ?? String(selected);
      return path;
    }
  } catch (err) {
    console.error("Folder dialog error:", err);
  }
  return null;
}

