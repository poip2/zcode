<script lang="ts">
  import { onMount, tick } from "svelte";
  import { document as docStore } from "$lib/stores/document";
  import { initRenderer, renderFull } from "$lib/renderer/pipeline";
  import {
    loadFile,
    saveFile,
    openFileDialog,
    getBaseDir,
    allowAssets,
  } from "$lib/tauri/files";
  import Editor from "$lib/components/Editor.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let rendererReady = $state(false);
  let isEditing = $state(false);
  let editContent = $state("");
  let dirty = $state(false);
  let statusMessage = $state("");

  onMount(() => {
    initRenderer();
    rendererReady = true;

    // Expose functions for native menu (Cmd+O from OS open events)
    (window as any).__zcode_open = () => handleOpenDialog();
    (window as any).__zcode_open_path = (path: string) => {
      if (path && rendererReady) loadFile(path);
    };

    window.addEventListener("keydown", handleKeydown);
    window.addEventListener("dragover", (e) => e.preventDefault());
    window.addEventListener("drop", handleDrop);

    return () => {
      window.removeEventListener("keydown", handleKeydown);
      window.removeEventListener("dragover", (e) => e.preventDefault());
      window.removeEventListener("drop", handleDrop);
    };
  });

  async function handleOpenDialog() {
    const path = await openFileDialog();
    if (path) await loadFile(path);
  }

  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (file) {
      // On Tauri desktop, the file path may be available via the webkitRelativePath or path property
      const path = (file as any).path;
      if (path) await loadFile(path);
    }
  }

  function toggleEdit() {
    const doc = $docStore;
    if (!doc.filePath) return;

    if (isEditing) {
      // Exiting edit mode — re-render from editContent to update preview
      if (dirty) {
        const baseDir = getBaseDir(doc.filePath);
        const result = renderFull(editContent, baseDir);
        allowAssets(result.assetPaths);

        docStore.set({
          ...doc,
          renderedHtml: result.html,
          frontmatter: result.frontmatter,
          wordCount: result.wordCount,
        });
      }
      isEditing = false;
    } else {
      editContent = doc.content;
      isEditing = true;
    }
  }

  async function handleSave() {
    const doc = $docStore;
    if (!doc.filePath || !dirty) return;

    try {
      await saveFile(doc.filePath, editContent);
      const baseDir = getBaseDir(doc.filePath);
      const result = renderFull(editContent, baseDir);
      await allowAssets(result.assetPaths);

      docStore.set({
        filePath: doc.filePath,
        fileName: doc.fileName,
        content: editContent,
        renderedHtml: result.html,
        frontmatter: result.frontmatter,
        wordCount: result.wordCount,
        loading: false,
        error: null,
      });

      dirty = false;
      isEditing = false;
      flashStatus("Saved");
    } catch (err) {
      console.error("Save failed:", err);
      flashStatus(`Save failed: ${err}`);
    }
  }

  let statusTimeout: ReturnType<typeof setTimeout> | undefined;

  function flashStatus(msg: string) {
    statusMessage = msg;
    clearTimeout(statusTimeout);
    statusTimeout = setTimeout(() => {
      statusMessage = "";
    }, 2000);
  }

  function handleKeydown(e: KeyboardEvent) {
    // Cmd+O: Open file
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "o") {
      e.preventDefault();
      handleOpenDialog();
      return;
    }
    // Cmd+S: Save
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "s") {
      e.preventDefault();
      if (dirty) handleSave();
      return;
    }
    // Cmd+E: Toggle edit/preview
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "e") {
      e.preventDefault();
      if ($docStore.filePath) toggleEdit();
      return;
    }
  }

  function handleEditChange(newValue: string) {
    editContent = newValue;
    dirty = newValue !== $docStore.content;
  }

  let doc = $derived($docStore);
</script>

<div class="app-root">
  {#if !rendererReady}
    <div class="state-center">
      <p class="state-text">Loading...</p>
    </div>
  {:else if doc.loading}
    <div class="state-center">
      <p class="state-text">Opening file...</p>
    </div>
  {:else if doc.error}
    <div class="state-center">
      <div class="error-box">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#ff3b30" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
        <p class="error-msg">{doc.error}</p>
        <button class="retry-btn" onclick={handleOpenDialog}>Open a file</button>
      </div>
    </div>
  {:else if doc.renderedHtml && isEditing}
    <Editor value={editContent} onChange={handleEditChange} />
  {:else if doc.renderedHtml}
    <div class="content-main">
      <MarkdownRenderer html={doc.renderedHtml} />
    </div>
  {:else}
    <div class="state-center">
      <div class="empty-state">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#aeaeb2" stroke-width="1" stroke-linecap="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
        <h2>Open a Markdown file</h2>
        <p class="hint">Press <kbd>⌘O</kbd> or drag a .md file here</p>
        <button class="open-btn" onclick={handleOpenDialog}>Open File...</button>
      </div>
    </div>
  {/if}

  <!-- Bottom status bar -->
  {#if doc.filePath || dirty || statusMessage}
    <div class="status-bar">
      {#if statusMessage}
        <span class="status-msg">{statusMessage}</span>
      {:else}
        <span class="status-file">{doc.fileName ?? ""}</span>
        {#if dirty}
          <span class="status-dirty">(unsaved)</span>
        {/if}
        {#if isEditing}
          <span class="status-mode">— Editing</span>
        {:else}
          <span class="status-mode">— Preview</span>
        {/if}
      {/if}
      <span class="status-hints">⌘O Open &nbsp; ⌘E Edit &nbsp; ⌘S Save</span>
    </div>
  {/if}
</div>

<style>
  .app-root {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    background: #fafafa;
    color: #1c1c1e;
  }

  .state-center {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 80vh;
  }

  .state-text {
    font-size: 14px;
    color: #aeaeb2;
  }

  .error-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
    max-width: 400px;
  }

  .error-msg {
    font-size: 13px;
    color: #8e8e93;
    line-height: 1.5;
  }

  .retry-btn {
    padding: 6px 16px;
    font-size: 13px;
    background: #f2f2f7;
    border: 1px solid #e5e5ea;
    border-radius: 6px;
    cursor: pointer;
    color: #1c1c1e;
  }

  .retry-btn:hover {
    background: #e5e5ea;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .empty-state h2 {
    font-size: 18px;
    font-weight: 600;
    color: #1c1c1e;
  }

  .hint {
    font-size: 13px;
    color: #8e8e93;
  }

  .hint kbd {
    display: inline-block;
    padding: 1px 6px;
    font-size: 12px;
    font-family: "SF Mono", monospace;
    background: #f2f2f7;
    border: 1px solid #e5e5ea;
    border-radius: 4px;
  }

  .open-btn {
    margin-top: 8px;
    padding: 8px 20px;
    font-size: 14px;
    font-weight: 500;
    background: #0891B2;
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
  }

  .open-btn:hover {
    background: #0E7490;
  }

  .content-main {
    flex: 1;
    padding-bottom: 40px;
  }

  .status-bar {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    font-size: 11px;
    background: #f2f2f7;
    border-top: 1px solid #e5e5ea;
    color: #8e8e93;
    font-family: -apple-system, sans-serif;
  }

  .status-file {
    font-weight: 500;
    color: #636366;
  }

  .status-dirty {
    color: #e67e22;
    margin-left: 4px;
  }

  .status-mode {
    color: #8e8e93;
    margin-left: 4px;
  }

  .status-msg {
    color: #0891B2;
    font-weight: 500;
  }

  .status-hints {
    color: #aeaeb2;
  }
</style>
