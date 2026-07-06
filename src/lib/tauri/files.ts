import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { document } from "../stores/document";
import { folderTree, type DirNode } from "../stores/folderTree";
import { recents } from "../stores/recents";
import { renderFull } from "../renderer/pipeline";

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
  const fileName = absolutePath.split("/").pop() ?? absolutePath;
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
    recents.addRecent(absolutePath);
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

export async function allowAssets(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  await invoke("allow_assets", { paths }).catch(() => {});
}

// ---- Folder tree commands ----

export async function listDirTree(rootPath: string): Promise<DirNode> {
  return invoke<DirNode>("read_dir_tree", { root: rootPath });
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

export async function refreshFolderTree() {
  const ft = folderTree;
  let root: string | null = null;
  ft.subscribe((s) => { root = s.rootPath; })();
  if (!root) return;
  ft.setLoading(true);
  try {
    const tree = await listDirTree(root);
    ft.setTree(tree);
  } catch (err) {
    ft.setError(`Failed to read folder: ${err}`);
  }
}
