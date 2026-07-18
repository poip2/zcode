<script lang="ts">
  import { onMount } from "svelte";
  import { document as docStore } from "$lib/stores/document";
  import { externalFile } from "$lib/stores/externalFile";
  import { initRenderer, renderFull } from "$lib/renderer/pipeline";
  import {
    loadFile,
    saveFile,
    openFileDialog,
    getBaseDir,
    allowAssets,
    reloadCurrentFile,
    openInShell,
    copyFileToFolder,
    getDefaultDataDir,
  } from "$lib/tauri/files";
  import { startFileWatcher, stopFileWatcher } from "$lib/tauri/watcher";
  import { load as loadSettings, resolveWorkspaceFolders } from "$lib/stores/settings";
  import { reloadSourcesFiles } from "$lib/stores/workspaceFiles";
  
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Editor from "$lib/components/Editor.svelte";
  import MarkdownRenderer from "$lib/components/MarkdownRenderer.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import AgentPanel from "$lib/components/AgentPanel.svelte";
  import AgentFab from "$lib/components/AgentFab.svelte";

  const SMALL_WINDOW_THRESHOLD = 640;

  let rendererReady = $state(false);
  let isEditing = $state(false);
  let editContent = $state("");
  let dirty = $state(false);
  let statusMessage = $state("");

  // Sidebar state
  let sidebarVisible = $state(true);
  let userCollapsed = $state(false);
  let settingsOpen = $state(false);
  let agentPanelOpen = $state(false);
  let lastWatchedPath = $state<string | null>(null);
  let dragHover = $state(false);
  let unlistenDragDrop: (() => void) | undefined;
  let unmounted = false;

  async function handleDroppedPaths(paths: string[]) {
    if (paths.length === 0) return;

    const settings = await loadSettings();
    const dataDir = await getDefaultDataDir();
    const { sourcesFolder } = await resolveWorkspaceFolders(settings, dataDir);

    let copiedCount = 0;
    let copyErrors = 0;

    for (const path of paths) {
      try {
        await copyFileToFolder(path, sourcesFolder);
        copiedCount++;
      } catch (err) {
        console.error("Failed to copy file to sources:", err);
        copyErrors++;
      }
    }

    await reloadSourcesFiles(sourcesFolder);
    const parts: string[] = [];
    if (copiedCount > 0) {
      parts.push(`Copied ${copiedCount} file${copiedCount === 1 ? "" : "s"} to Sources`);
    }
    if (copyErrors > 0) {
      parts.push(`${copyErrors} cop${copyErrors === 1 ? "y" : "ies"} failed`);
    }
    if (parts.length > 0) {
      flashStatus(parts.join(" — "));
    }
  }

  onMount(() => {
    initRenderer();
    rendererReady = true;

    (window as any).__zcode_open = () => handleOpenDialog();
    (window as any).__zcode_open_path = (path: string) => {
      if (path && rendererReady) loadFile(path);
    };

    window.addEventListener("keydown", handleKeydown);

    // Native Tauri drag-and-drop (cross-platform, replaces DOM drop events).
    // Setup is async so we fire-and-store the unlisten promise for cleanup.
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          dragHover = true;
        } else if (event.payload.type === "drop") {
          dragHover = false;
          handleDroppedPaths(event.payload.paths).catch(err =>
            console.error('Drag-drop error:', err));
        } else {
          dragHover = false;
        }
      })
      .then((fn) => {
        if (unmounted) {
          fn();
        } else {
          unlistenDragDrop = fn;
        }
      })
      .catch((err) => {
        console.error("Failed to register drag-drop listener:", err);
      });

    // Window resize listener for auto-collapse
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    function handleResize() {
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        const w = window.innerWidth;
        if (w < SMALL_WINDOW_THRESHOLD && sidebarVisible && !userCollapsed) {
          sidebarVisible = false;
        } else if (w >= SMALL_WINDOW_THRESHOLD && !sidebarVisible && !userCollapsed) {
          sidebarVisible = true;
        }
      }, 100);
    }
    window.addEventListener("resize", handleResize);

    return () => {
      unmounted = true;
      window.removeEventListener("keydown", handleKeydown);
      window.removeEventListener("resize", handleResize);
      unlistenDragDrop?.();
      stopFileWatcher();
    };
  });

  function toggleSidebar() {
    if (sidebarVisible) {
      // User is manually hiding
      sidebarVisible = false;
      userCollapsed = true;
    } else {
      // User is manually showing
      sidebarVisible = true;
      userCollapsed = false;
    }
  }

  async function handleOpenDialog() {
    const path = await openFileDialog();
    if (path) await loadFile(path);
  }

  function toggleEdit() {
    const doc = $docStore;
    if (!doc.filePath) return;

    if (isEditing) {
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
      if (!dirty) {
        editContent = doc.content;
      }
      isEditing = true;
    }
  }

  async function handleSave() {
    const doc = $docStore;
    if (!doc.filePath || !dirty) return;

    try {
      await saveFile(doc.filePath, editContent);

      if ($docStore.filePath !== doc.filePath) {
        return;
      }

      await reloadCurrentFile(doc.filePath, true);

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
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "o") {
      e.preventDefault();
      handleOpenDialog();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "s") {
      e.preventDefault();
      if (dirty) handleSave();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "e") {
      e.preventDefault();
      if ($docStore.filePath) toggleEdit();
      return;
    }
    // Cmd+B: toggle sidebar
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "b") {
      e.preventDefault();
      toggleSidebar();
      return;
    }
  }

  function handleEditChange(newValue: string) {
    editContent = newValue;
    dirty = newValue !== $docStore.content;
  }

  // Watch file path changes to manage the watcher lifecycle
  $effect(() => {
    const path = $docStore.filePath;
    if (path && path !== lastWatchedPath) {
      lastWatchedPath = path;
      startFileWatcher(path);
    }
  });

  // When file content changes externally (via watcher), sync editor if not editing
  $effect(() => {
    const newContent = $docStore.content;
    if (isEditing && !dirty && editContent !== newContent) {
      editContent = newContent;
    }
  });

  let doc = $derived($docStore);

  // Clear externalFile when a markdown file is loaded
  $effect(() => {
    if (doc.filePath) {
      externalFile.set(null);
    }
  });
</script>

<div class="app-root">
  <TitleBar {sidebarVisible} onToggleSidebar={toggleSidebar} onOpenSettings={() => (settingsOpen = true)} />

  <div class="app-body">
    {#if sidebarVisible}
      <Sidebar />
    {/if}

    <main class="main-pane">
      {#if !rendererReady}
        <div class="state-center">
          <p class="state-text">Loading…</p>
        </div>
      {:else if doc.loading}
        <div class="state-center">
          <p class="state-text">Opening file…</p>
        </div>
      {:else if doc.error}
        <div class="state-center">
          <div class="error-box">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#e67e22" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
            <p class="error-msg">{doc.error}</p>
            <button class="retry-btn" onclick={handleOpenDialog}>Open a file</button>
          </div>
        </div>
      {:else if $externalFile}
        <div class="state-center">
          <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#aeaeb2" stroke-width="1" stroke-linecap="round"><rect x="3" y="4" width="18" height="16" rx="2"/><line x1="8" y1="2" x2="16" y2="2"/><line x1="12" y1="11" x2="12" y2="17"/><polyline points="8 14 12 18 16 14"/></svg>
            <h2>This file type isn't previewable here</h2>
            <p class="hint">{$externalFile.name}</p>
            <button class="open-btn" onclick={() => openInShell($externalFile!.path)}>Open in default app</button>
          </div>
        </div>
      {:else if doc.filePath && isEditing}
        <Editor value={editContent} onChange={handleEditChange} />
      {:else if doc.filePath}
        <div class="content-main">
          <MarkdownRenderer html={doc.renderedHtml} />
        </div>
      {:else}
        <div class="state-center">
          <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#aeaeb2" stroke-width="1" stroke-linecap="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
            <h2>Open a Markdown file</h2>
            <p class="hint">Press <kbd>⌘O</kbd> or drop files here to copy to Sources</p>
            <button class="open-btn" onclick={handleOpenDialog}>Open File…</button>
          </div>
        </div>
      {/if}

      <!-- Status bar -->
      {#if doc.filePath || dirty || statusMessage}
        <div class="status-bar">
          {#if statusMessage}
            <span class="status-msg">{statusMessage}</span>
          {:else}
            <span class="status-file">{doc.fileName ?? ""}</span>
            {#if dirty}
              <span class="status-dirty">(unsaved)</span>
            {/if}
            <span class="status-mode">
              {isEditing ? "— Editing" : "— Preview"}
            </span>
          {/if}
          <span class="status-hints">
            {#if isEditing}
              <span class="hint-full">⌘O Open &nbsp; ⌘E Preview &nbsp; ⌘S Save &nbsp; ⌘B Sidebar</span>
            {:else}
              <span class="hint-full">⌘O Open &nbsp; ⌘E Edit &nbsp; ⌘S Save &nbsp; ⌘B Sidebar</span>
            {/if}
            <span class="hint-compact">⌘O &nbsp; ⌘E &nbsp; ⌘S &nbsp; ⌘B</span>
          </span>
        </div>
      {/if}
    </main>
  </div>

  <SettingsDialog open={settingsOpen} onClose={() => (settingsOpen = false)} />

  {#if dragHover}
    <div class="drag-overlay">
      <div class="drag-hint">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <rect x="3" y="4" width="18" height="16" rx="2"/>
          <line x1="8" y1="2" x2="16" y2="2"/>
          <line x1="12" y1="11" x2="12" y2="17"/>
          <polyline points="8 14 12 18 16 14"/>
        </svg>
        <p>Drop files to copy to Sources</p>
      </div>
    </div>
  {/if}

</div>

<!-- Floating AI Agent (outside layout flow) -->
<AgentFab open={agentPanelOpen} onclick={() => (agentPanelOpen = !agentPanelOpen)} />
{#if agentPanelOpen}
  <AgentPanel filePath={doc?.filePath ?? null} onClose={() => (agentPanelOpen = false)} />
{/if}

<style>
  .app-root {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--zc-bg-page, #FAF9F6);
    color: var(--zc-text-primary, #1F1E1C);
    overflow: hidden;
  }

  .app-body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  /* Drag overlay */
  .drag-overlay {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: rgba(24, 21, 16, 0.12);
    backdrop-filter: blur(2px);
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }

  .drag-hint {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    background: var(--zc-bg-card, #FDFDFB);
    border: 2px dashed var(--zc-border, #E7E4DD);
    border-radius: 12px;
    padding: 32px 48px;
    text-align: center;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .drag-hint svg {
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .drag-hint p {
    font-size: 13px;
    line-height: 1.6;
    color: var(--zc-text-secondary, #8A8782);
    margin: 0;
  }

  .main-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--zc-bg-chrome, #F4F2ED);
    overflow-y: auto;
    container-type: inline-size;
    container-name: mainpane;
  }

  .state-center {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    flex: 1;
  }

  .state-text {
    font-size: 14px;
    color: var(--zc-text-tertiary, #A8A49D);
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
    color: var(--zc-text-secondary, #8A8782);
    line-height: 1.5;
  }

  .retry-btn {
    padding: 6px 16px;
    font-size: 13px;
    background: #f2f2f7;
    border: 1px solid #e5e5ea;
    border-radius: 6px;
    cursor: pointer;
    color: var(--zc-text-primary, #1F1E1C);
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
    color: var(--zc-text-primary, #1F1E1C);
  }

  .hint {
    font-size: 13px;
    color: var(--zc-text-secondary, #8A8782);
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
    background: var(--zc-text-primary, #1F1E1C);
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
  }

  .open-btn:hover {
    opacity: 0.9;
  }

  .content-main {
    flex: 1;
    padding-bottom: 40px;
  }

  .status-bar {
    min-height: 28px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    row-gap: 2px;
    padding: 5px 12px;
    font-size: 11px;
    background: var(--zc-bg-chrome, #F4F2ED);
    border-top: 1px solid var(--zc-border, #E7E4DD);
    color: var(--zc-text-secondary, #8A8782);
    font-family: -apple-system, sans-serif;
    flex-shrink: 0;
  }

  .status-file {
    font-weight: 500;
    color: var(--zc-text-secondary, #8A8782);
  }

  .status-dirty {
    color: #e67e22;
    margin-left: 4px;
  }

  .status-mode {
    color: var(--zc-text-secondary, #8A8782);
    margin-left: 4px;
  }

  .status-msg {
    color: var(--zc-text-secondary, #8A8782);
    font-weight: 500;
  }

  .status-hints {
    color: var(--zc-text-tertiary, #A8A49D);
    white-space: nowrap;
  }

  .status-hints .hint-compact {
    display: none;
  }

  @container mainpane (max-width: 460px) {
    .status-hints .hint-full {
      display: none;
    }
    .status-hints .hint-compact {
      display: inline;
    }
  }
</style>
