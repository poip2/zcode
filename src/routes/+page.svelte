<script lang="ts">
  import { onMount, tick } from "svelte";
  import { document as docStore } from "$lib/stores/document";
  import { externalFile } from "$lib/stores/externalFile";
  import { initRenderer, renderFull } from "$lib/renderer/pipeline";
  import {
    loadFile,
    cancelPendingFileLoad,
    saveFile,
    openFileDialog,
    getBaseDir,
    allowAssets,
    reloadCurrentFile,
    openInShell,
    copyFileToFolder,
    getDefaultDataDir,
    printCurrentWebview,
  } from "$lib/tauri/files";
  import { startFileWatcher, stopFileWatcher } from "$lib/tauri/watcher";
  import { load as loadSettings, resolveWorkspaceFolders } from "$lib/stores/settings";
  import { reloadSourcesFiles } from "$lib/stores/workspaceFiles";
  
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t, tt } from "$lib/i18n";
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
  let watcherStartingPath = $state<string | null>(null);
  let watcherFailedPath = $state<string | null>(null);
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
      parts.push(tt(copiedCount === 1 ? 'editor.copiedOne' : 'editor.copied', { count: copiedCount }));
    }
    if (copyErrors > 0) {
      parts.push(tt(copyErrors === 1 ? 'editor.copyFailedOne' : 'editor.copyFailed', { count: copyErrors }));
    }
    if (parts.length > 0) {
      flashStatus(parts.join(tt('editor.sep')));
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

  function normalizePath(path: string): string {
    const normalized = path.replace(/\\/g, "/").replace(/\/$/, "");
    return navigator.platform.includes("Win") ? normalized.toLowerCase() : normalized;
  }

  function remapPath(currentPath: string, oldBase: string, newBase: string): string | null {
    const current = normalizePath(currentPath);
    const oldPath = normalizePath(oldBase);
    if (current !== oldPath && !current.startsWith(`${oldPath}/`)) return null;

    const normalizedCurrent = currentPath.replace(/\\/g, "/");
    const normalizedOld = oldBase.replace(/\\/g, "/").replace(/\/$/, "");
    const suffix = normalizedCurrent.slice(normalizedOld.length);
    const separator = newBase.includes("\\") ? "\\" : "/";
    return newBase + suffix.replaceAll("/", separator);
  }

  function fileNameFromPath(path: string): string {
    return path.replace(/\\/g, "/").split("/").pop() ?? path;
  }

  function handleEntryPathChanged(oldPath: string, newPath: string) {
    const currentDoc = $docStore;
    if (currentDoc.filePath) {
      const mapped = remapPath(currentDoc.filePath, oldPath, newPath);
      if (mapped) {
        cancelPendingFileLoad();
        if (currentDoc.loading) {
          void loadFile(mapped).catch((err) => console.error("Failed to reload moved file:", err));
        } else {
          const preview = renderFull(dirty ? editContent : currentDoc.content, getBaseDir(mapped));
          allowAssets(preview.assetPaths).catch(() => {});
          docStore.set({
            ...currentDoc,
            filePath: mapped,
            fileName: fileNameFromPath(mapped),
            renderedHtml: preview.html,
            frontmatter: preview.frontmatter,
            wordCount: preview.wordCount,
          });
          getCurrentWindow().setTitle(`${fileNameFromPath(mapped)} — zcode`).catch(() => {});
        }
      }
    }

    const currentExternal = $externalFile;
    if (currentExternal) {
      const mapped = remapPath(currentExternal.path, oldPath, newPath);
      if (mapped) externalFile.set({ path: mapped, name: fileNameFromPath(mapped) });
    }
  }

  function handleEntryDeleted(path: string) {
    const currentDoc = $docStore;
    if (currentDoc.filePath && remapPath(currentDoc.filePath, path, path)) {
      cancelPendingFileLoad();
      stopFileWatcher();
      lastWatchedPath = null;
      watcherStartingPath = null;
      watcherFailedPath = null;
      isEditing = false;
      editContent = "";
      dirty = false;
      docStore.set({
        filePath: null,
        fileName: null,
        content: "",
        renderedHtml: "",
        frontmatter: null,
        wordCount: 0,
        loading: false,
        error: null,
      });
      getCurrentWindow().setTitle("zcode").catch(() => {});
    }
    if ($externalFile?.path && remapPath($externalFile.path, path, path)) {
      cancelPendingFileLoad();
      externalFile.set(null);
    }
  }

  async function waitForPrintableAssets() {
    await window.document.fonts?.ready.catch(() => {});
    const pendingImages = [...window.document.querySelectorAll<HTMLImageElement>(".md-content img")]
      .filter((image) => !image.complete)
      .map((image) => new Promise<void>((resolve) => {
        image.addEventListener("load", () => resolve(), { once: true });
        image.addEventListener("error", () => resolve(), { once: true });
      }));
    if (pendingImages.length === 0) return;
    await Promise.race([
      Promise.all(pendingImages),
      new Promise<void>((resolve) => setTimeout(resolve, 2500)),
    ]);
  }

  async function handleExportPdf() {
    const currentDoc = $docStore;
    if (!currentDoc.filePath || currentDoc.loading || currentDoc.error) return;

    if (dirty) {
      const result = renderFull(editContent, getBaseDir(currentDoc.filePath));
      await allowAssets(result.assetPaths);
      docStore.set({
        ...currentDoc,
        renderedHtml: result.html,
        frontmatter: result.frontmatter,
        wordCount: result.wordCount,
      });
    }
    isEditing = false;
    await tick();
    await waitForPrintableAssets();
    try {
      if (navigator.platform.includes("Mac")) {
        await printCurrentWebview();
      } else {
        window.print();
      }
    } catch (err) {
      console.error("PDF export failed:", err);
      flashStatus(tt("editor.exportPdfFailed", { error: String(err) }));
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
      if ($docStore.filePath !== doc.filePath) return;

      dirty = false;
      isEditing = false;
      flashStatus(tt('editor.saved'));
    } catch (err) {
      console.error("Save failed:", err);
      flashStatus(tt('editor.saveFailed', { error: String(err) }));
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
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "p") {
      e.preventDefault();
      handleExportPdf();
      return;
    }
  }

  function handleEditChange(newValue: string) {
    editContent = newValue;
    dirty = newValue !== $docStore.content;
  }

  async function startWatcherWithRetry(path: string) {
    let lastError: unknown;
    try {
      for (const delay of [0, 250, 1000]) {
        if (unmounted || $docStore.filePath !== path) return;
        if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
        if (unmounted || $docStore.filePath !== path) return;
        try {
          await startFileWatcher(path);
          if ($docStore.filePath === path) {
            lastWatchedPath = path;
            watcherFailedPath = null;
          }
          return;
        } catch (err) {
          lastError = err;
        }
      }
      if ($docStore.filePath === path) {
        watcherFailedPath = path;
        console.error("Failed to watch file after retries:", lastError);
      }
    } finally {
      if (watcherStartingPath === path) watcherStartingPath = null;
    }
  }

  // Watch file path changes to manage the watcher lifecycle.
  $effect(() => {
    const state = $docStore;
    const path = state.filePath;
    if (state.loading && watcherFailedPath === path) watcherFailedPath = null;
    if (
      path &&
      !state.loading &&
      !state.error &&
      path !== lastWatchedPath &&
      path !== watcherStartingPath &&
      path !== watcherFailedPath
    ) {
      watcherStartingPath = path;
      void startWatcherWithRetry(path);
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
  <TitleBar
    {sidebarVisible}
    onToggleSidebar={toggleSidebar}
    onOpenSettings={() => (settingsOpen = true)}
    onExportPdf={handleExportPdf}
    canExportPdf={Boolean(doc.filePath && !doc.loading && !doc.error)}
  />

  <div class="app-body">
    {#if sidebarVisible}
      <Sidebar
        {dirty}
        onEntryPathChanged={handleEntryPathChanged}
        onEntryDeleted={handleEntryDeleted}
        onStatus={flashStatus}
      />
    {/if}

    <main class="main-pane">
      {#if !rendererReady}
        <div class="state-center">
          <p class="state-text">{$t('editor.loading')}</p>
        </div>
      {:else if doc.loading}
        <div class="state-center">
          <p class="state-text">{$t('editor.opening')}</p>
        </div>
      {:else if doc.error}
        <div class="state-center">
          <div class="error-box">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#e67e22" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
            <p class="error-msg">{doc.error}</p>
            <button class="retry-btn" onclick={handleOpenDialog}>{$t('editor.openFile')}</button>
          </div>
        </div>
      {:else if $externalFile}
        <div class="state-center">
          <div class="empty-state">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#aeaeb2" stroke-width="1" stroke-linecap="round"><rect x="3" y="4" width="18" height="16" rx="2"/><line x1="8" y1="2" x2="16" y2="2"/><line x1="12" y1="11" x2="12" y2="17"/><polyline points="8 14 12 18 16 14"/></svg>
            <h2>{$t('editor.notPreviewable')}</h2>
            <p class="hint">{$externalFile.name}</p>
            <button class="open-btn" onclick={() => openInShell($externalFile!.path)}>{$t('editor.openInApp')}</button>
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
            <h2>{$t('editor.openMarkdown')}</h2>
            <p class="hint">{@html $t('editor.openHint', { shortcut: '<kbd>⌘O</kbd>' })}</p>
            <button class="open-btn" onclick={handleOpenDialog}>{$t('editor.openFileBtn')}</button>
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
              <span class="status-dirty">{$t('editor.unsaved')}</span>
            {/if}
            <span class="status-mode">
              {isEditing ? $t('editor.editing') : $t('editor.preview')}
            </span>
          {/if}
          <span class="status-hints">
            {#if isEditing}
              <span class="hint-full">{$t('editor.shortcuts.editing')}</span>
            {:else}
              <span class="hint-full">{$t('editor.shortcuts.preview')}</span>
            {/if}
            <span class="hint-compact">{$t('editor.shortcuts.compact')}</span>
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
        <p>{$t('editor.dropHint')}</p>
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

  .hint :global(kbd) {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 11px;
    padding: 1px 5px;
    background: var(--zc-bg-chrome, #F4F2ED);
    border: 1px solid var(--zc-border, #E7E4DD);
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

  @media print {
    @page {
      size: A4;
      margin: 16mm 18mm;
    }

    :global(html),
    :global(body) {
      width: auto !important;
      height: auto !important;
      overflow: visible !important;
      background: white !important;
    }

    :global(.titlebar),
    :global(.sidebar),
    :global(.status-bar),
    :global(.agent-fab),
    :global(.agent-panel),
    :global(.code-copy-btn) {
      display: none !important;
    }

    .app-root,
    .app-body,
    .main-pane,
    .content-main {
      display: block !important;
      width: auto !important;
      height: auto !important;
      min-height: 0 !important;
      overflow: visible !important;
      margin: 0 !important;
      padding: 0 !important;
      background: white !important;
    }

    :global(.md-content) {
      max-width: none !important;
      margin: 0 !important;
      padding: 0 !important;
      color: #111 !important;
      print-color-adjust: exact;
      -webkit-print-color-adjust: exact;
    }

    :global(.md-content pre),
    :global(.md-content blockquote),
    :global(.md-content table),
    :global(.md-content img) {
      break-inside: avoid;
    }
  }
</style>
