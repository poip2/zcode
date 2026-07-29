<script module lang="ts">
  let autoLoadDone = false;
</script>

<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { get } from "svelte/store";
  import { document as docStore } from "$lib/stores/document";
  import { folderTree, type DirNode } from "$lib/stores/folderTree";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { externalFile } from "$lib/stores/externalFile";
  import {
    loadFile,
    cancelPendingFileLoad,
    openFolderDialog,
    listDirTree,
    createMarkdownFile,
    createFolder,
    renamePath,
    moveDocument,
    trashPath,
    pathExists,
    openInShell,
    getDefaultDataDir,
  } from "$lib/tauri/files";
  import { load as loadSettings, onSettingsChange, resolveWorkspaceFolders } from "$lib/stores/settings";
  import { sourcesFiles, outputFiles, reloadSourcesFiles, reloadOutputFiles } from "$lib/stores/workspaceFiles";
  import { isMarkdownExt } from "$lib/utils/fileTypes";
  import { t, tt } from "$lib/i18n";

  let {
    dirty = false,
    onEntryPathChanged = (_oldPath: string, _newPath: string) => {},
    onEntryDeleted = (_path: string) => {},
    onStatus = (_message: string) => {},
  }: {
    dirty?: boolean;
    onEntryPathChanged?: (oldPath: string, newPath: string) => void;
    onEntryDeleted?: (path: string) => void;
    onStatus?: (message: string) => void;
  } = $props();

  let newItemMode = $state<null | "file" | "folder">(null);
  let newItemName = $state("");
  let newItemError = $state("");
  let newItemInput: HTMLInputElement | undefined = $state();

  let selectedFolder = $state<string | null>(null);
  let sourcesExpanded = $state(false);
  let outputExpanded = $state(false);
  let sourcesFolderPath = $state("");
  let outputFolderPath = $state("");

  let contextMenu = $state<{ node: DirNode; x: number; y: number } | null>(null);
  let renamingPath = $state<string | null>(null);
  let renameName = $state("");
  let renameInput: HTMLInputElement | undefined = $state();
  let renameBusy = $state(false);
  let operationError = $state("");
  let draggedPath = $state<string | null>(null);
  let dropTargetPath = $state<string | null>(null);

  let doc = $derived($docStore);
  let ft = $derived($folderTree);
  let expanded = $derived(folderTree.expanded);
  let pinnedPath = $derived($pinnedFolder);
  let hasFolderSelected = $derived(selectedFolder !== null);

  function normalizePath(path: string): string {
    const normalized = path.replace(/\\/g, "/").replace(/\/$/, "");
    return navigator.platform.includes("Win") ? normalized.toLowerCase() : normalized;
  }

  function pathContains(parent: string, child: string): boolean {
    const base = normalizePath(parent);
    const candidate = normalizePath(child);
    return candidate === base || candidate.startsWith(`${base}/`);
  }

  function affectsOpenEntry(path: string): boolean {
    return Boolean(
      (doc.filePath && pathContains(path, doc.filePath)) ||
      ($externalFile?.path && pathContains(path, $externalFile.path)),
    );
  }

  function clearMenus(event?: Event) {
    const target = event?.target;
    if (target instanceof Element && (target.closest(".context-menu") || target.closest(".rename-input"))) return;
    contextMenu = null;
  }

  onMount(async () => {
    window.addEventListener("pointerdown", clearMenus);
    window.addEventListener("blur", clearMenus);
    await pinnedFolder.load();
    await reloadWorkspaceFiles();

    if (!autoLoadDone) {
      const p = $pinnedFolder;
      if (p) {
        const exists = await pathExists(p).catch(() => false);
        if (exists) {
          autoLoadDone = true;
          const current = get(folderTree);
          if (current.rootPath === p && current.tree !== null) return;
          folderTree.setRoot(p);
          folderTree.setLoading(true);
          try {
            folderTree.setTree(await listDirTree(p));
          } catch (err) {
            folderTree.setError(tt("sidebar.failedReadPinned", { error: String(err) }));
          }
        }
      }
    }
  });

  onDestroy(() => {
    window.removeEventListener("pointerdown", clearMenus);
    window.removeEventListener("blur", clearMenus);
  });

  onDestroy(onSettingsChange(() => {
    const p = get(pinnedFolder);
    if (p && p !== ft.rootPath) openFolderPath(p).catch(() => {});
    reloadWorkspaceFiles().catch(() => {});
  }));

  $effect(() => {
    if (doc.filePath) selectedFolder = null;
  });

  async function reloadWorkspaceFiles() {
    const dataDir = await getDefaultDataDir();
    const settings = await loadSettings();
    const resolved = await resolveWorkspaceFolders(settings, dataDir);
    sourcesFolderPath = resolved.sourcesFolder;
    outputFolderPath = resolved.outputFolder;
    await reloadSourcesFiles(sourcesFolderPath);
    await reloadOutputFiles(outputFolderPath);
  }

  function selectFolder(path: string) {
    selectedFolder = path;
  }

  function startNew(mode: "file" | "folder") {
    newItemMode = mode;
    newItemName = "";
    newItemError = "";
    requestAnimationFrame(() => newItemInput?.focus());
  }

  function cancelNew() {
    newItemMode = null;
    newItemName = "";
    newItemError = "";
  }

  async function confirmNew() {
    const name = newItemName.trim();
    if (!name) {
      cancelNew();
      return;
    }
    const dir = selectedFolder ?? ft.rootPath;
    if (!dir) return;

    if (selectedFolder) folderTree.expanded.ensure(selectedFolder);
    try {
      if (newItemMode === "file") {
        const createdPath = await createMarkdownFile(dir, name);
        await refreshTree();
        if (isMarkdownExt(createdPath)) await loadFile(createdPath);
      } else {
        await createFolder(dir, name);
        await refreshTree();
      }
      cancelNew();
    } catch (err) {
      newItemError = String(err);
    }
  }

  function handleNewKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      confirmNew();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelNew();
    }
  }

  async function handleOpenFolder() {
    const path = await openFolderDialog();
    if (path) await openFolderPath(path);
  }

  async function openFolderPath(path: string) {
    folderTree.setRoot(path);
    folderTree.setLoading(true);
    try {
      folderTree.setTree(await listDirTree(path));
    } catch (err) {
      folderTree.setError(tt("sidebar.failedReadFolder", { error: String(err) }));
    }
  }

  function handlePin() {
    const path = ft.rootPath;
    if (!path) return;
    if (pinnedPath === path) pinnedFolder.unpin();
    else pinnedFolder.pin(path);
  }

  async function refreshTree() {
    await folderTree.refresh();
  }

  function handleFileClick(node: DirNode) {
    selectedFolder = null;
    if (isMarkdownExt(node.name)) {
      externalFile.set(null);
      loadFile(node.path);
    } else {
      cancelPendingFileLoad();
      externalFile.set({ path: node.path, name: node.name });
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
    }
  }

  function handleFileKeydown(event: KeyboardEvent, node: DirNode) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      handleFileClick(node);
    }
  }

  function toggleDir(path: string) {
    folderTree.expanded.toggle(path);
  }

  function openContextMenu(event: MouseEvent, node: DirNode) {
    event.preventDefault();
    event.stopPropagation();
    const menuWidth = 160;
    const menuHeight = 88;
    contextMenu = {
      node,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight - 8)),
    };
    selectedFolder = node.is_dir ? node.path : null;
    operationError = "";
  }

  async function startRename(node: DirNode) {
    contextMenu = null;
    renamingPath = node.path;
    renameName = node.name;
    operationError = "";
    await tick();
    renameInput?.focus();
    const extensionAt = node.is_dir ? -1 : node.name.lastIndexOf(".");
    renameInput?.setSelectionRange(0, extensionAt > 0 ? extensionAt : node.name.length);
  }

  function cancelRename() {
    if (renameBusy) return;
    renamingPath = null;
    renameName = "";
  }

  async function confirmRename(node: DirNode) {
    if (renameBusy || renamingPath !== node.path) return;
    const name = renameName.trim();
    if (!name || name === node.name) {
      cancelRename();
      return;
    }

    renameBusy = true;
    operationError = "";
    let failed = false;
    try {
      const newPath = await renamePath(node.path, name);
      if (node.is_dir) folderTree.expanded.replaceTree(node.path, newPath);
      if (selectedFolder && pathContains(node.path, selectedFolder)) {
        selectedFolder = newPath + selectedFolder.slice(node.path.length);
      }
      onEntryPathChanged(node.path, newPath);
      renamingPath = null;
      renameName = "";
      await refreshTree();
      onStatus(tt("sidebar.renamed"));
    } catch (err) {
      failed = true;
      operationError = String(err);
    } finally {
      renameBusy = false;
    }
    if (failed) {
      await tick();
      renameInput?.focus();
    }
  }

  function handleRenameKeydown(event: KeyboardEvent, node: DirNode) {
    if (event.key === "Enter") {
      event.preventDefault();
      confirmRename(node);
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelRename();
    }
  }

  async function deleteEntry(node: DirNode) {
    contextMenu = null;
    const warningKey = dirty && affectsOpenEntry(node.path)
      ? "sidebar.deleteConfirmUnsaved"
      : node.is_dir
        ? "sidebar.deleteFolderConfirm"
        : "sidebar.deleteFileConfirm";
    if (!window.confirm(tt(warningKey, { name: node.name }))) return;

    operationError = "";
    try {
      await trashPath(node.path);
      folderTree.expanded.removeTree(node.path);
      if (selectedFolder && pathContains(node.path, selectedFolder)) selectedFolder = null;
      onEntryDeleted(node.path);
      await refreshTree();
      onStatus(tt("sidebar.movedToTrash"));
    } catch (err) {
      operationError = String(err);
    }
  }

  function startDocumentDrag(event: DragEvent, node: DirNode) {
    draggedPath = node.path;
    event.dataTransfer?.setData("text/plain", node.path);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function allowFolderDrop(event: DragEvent, folderPath: string) {
    if (!draggedPath || pathContains(draggedPath, folderPath)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
    dropTargetPath = folderPath;
  }

  async function dropDocument(event: DragEvent, folderPath: string) {
    event.preventDefault();
    const sourcePath = draggedPath ?? event.dataTransfer?.getData("text/plain");
    draggedPath = null;
    dropTargetPath = null;
    if (!sourcePath) return;

    operationError = "";
    try {
      const newPath = await moveDocument(sourcePath, folderPath);
      folderTree.expanded.ensure(folderPath);
      onEntryPathChanged(sourcePath, newPath);
      await refreshTree();
      onStatus(tt("sidebar.moved"));
    } catch (err) {
      operationError = String(err);
    }
  }

  function finishDocumentDrag() {
    draggedPath = null;
    dropTargetPath = null;
  }
</script>

{#snippet treeNode(node: DirNode, depth: number)}
  {@const rowPadding = 14 + depth * 16}
  {#if node.is_dir}
    {@const open = $expanded.has(node.path)}
    {@const hasKids = Boolean(node.children?.length)}
    <div
      class="tree-row"
      role="group"
      class:selected={selectedFolder === node.path}
      class:drop-target={dropTargetPath === node.path}
      style={`padding-left: ${rowPadding}px`}
      oncontextmenu={(event) => openContextMenu(event, node)}
      ondragover={(event) => allowFolderDrop(event, node.path)}
      ondragleave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) dropTargetPath = null;
      }}
      ondrop={(event) => dropDocument(event, node.path)}
    >
      {#if hasKids}
        <button
          class="tree-chevron"
          onclick={() => toggleDir(node.path)}
          aria-expanded={open}
          aria-label={open ? $t("sidebar.collapseFolder") : $t("sidebar.expandFolder")}
          data-tauri-drag-region="false"
        >
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" class="chevron-svg" class:rotated={open}>
            <polyline points="6,3 11,8 6,13" />
          </svg>
        </button>
      {:else}
        <span class="tree-chevron-placeholder"></span>
      {/if}

      {#if renamingPath === node.path}
        <span class="tree-icon">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
            <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z" />
          </svg>
        </span>
        <input
          bind:this={renameInput}
          bind:value={renameName}
          class="rename-input"
          disabled={renameBusy}
          onkeydown={(event) => handleRenameKeydown(event, node)}
          onblur={() => confirmRename(node)}
        />
      {:else}
        <button class="tree-folder-label" onclick={() => selectFolder(node.path)} data-tauri-drag-region="false">
          <span class="tree-icon">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
              <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z" />
            </svg>
          </span>
          <span class="tree-label">{node.name}</span>
        </button>
      {/if}
    </div>

    {#if hasKids && open}
      {#each node.children ?? [] as child (child.path)}
        {@render treeNode(child, depth + 1)}
      {/each}
    {/if}
  {:else if renamingPath === node.path}
    <div class="tree-row tree-file" style={`padding-left: ${rowPadding}px`}>
      <span class="tree-chevron-placeholder"></span>
      <span class="tree-icon file-icon">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
          <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z" />
          <polyline points="10,2 10,6 14,6" />
        </svg>
      </span>
      <input
        bind:this={renameInput}
        bind:value={renameName}
        class="rename-input"
        disabled={renameBusy}
        onkeydown={(event) => handleRenameKeydown(event, node)}
        onblur={() => confirmRename(node)}
      />
    </div>
  {:else}
    <button
      class="tree-row tree-file"
      class:active={(doc.filePath === node.path || $externalFile?.path === node.path) && !hasFolderSelected}
      class:dragging={draggedPath === node.path}
      style={`padding-left: ${rowPadding}px`}
      onclick={() => handleFileClick(node)}
      onkeydown={(event) => handleFileKeydown(event, node)}
      oncontextmenu={(event) => openContextMenu(event, node)}
      draggable="true"
      ondragstart={(event) => startDocumentDrag(event, node)}
      ondragend={finishDocumentDrag}
      data-tauri-drag-region="false"
    >
      <span class="tree-chevron-placeholder"></span>
      <span class="tree-icon file-icon">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
          <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z" />
          <polyline points="10,2 10,6 14,6" />
        </svg>
      </span>
      <span class="tree-label">{node.name}</span>
    </button>
  {/if}
{/snippet}

<div class="sidebar">
  <div class="sidebar-header">
    <span class="sidebar-title">{$t("sidebar.title")}</span>
    <div class="sidebar-actions">
      {#if ft.rootPath}
        <button
          class="sb-icon-btn"
          class:is-pinned={pinnedPath === ft.rootPath}
          title={pinnedPath === ft.rootPath ? $t("sidebar.unpinFolder") : $t("sidebar.pinFolder")}
          onclick={handlePin}
          data-tauri-drag-region="false"
        >
          {#if pinnedPath === ft.rootPath}
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" stroke="none">
              <path d="M6.5 1.5a.5.5 0 0 1 .5.5v4l2.5 2.5V14l-2-2-2 2V8.5L8 6V2a.5.5 0 0 1 .5-.5h-2z" />
              <path d="M9.5 1.5L5.5 12" stroke="currentColor" stroke-width="1.5" fill="none" />
            </svg>
          {:else}
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
              <path d="M6.5 1.5v4L9 8v6l-2-2-2 2V8l2.5-2.5v-4" />
              <line x1="5" y1="1.5" x2="10" y2="1.5" />
            </svg>
          {/if}
        </button>
      {/if}
      <button class="sb-icon-btn" title={$t("sidebar.newFile")} onclick={() => startNew("file")} data-tauri-drag-region="false">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
          <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z" />
          <line x1="8" y1="11" x2="8" y2="7" /><line x1="6" y1="9" x2="10" y2="9" />
        </svg>
      </button>
      <button class="sb-icon-btn" title={$t("sidebar.newFolder")} onclick={() => startNew("folder")} data-tauri-drag-region="false">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
          <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z" />
          <line x1="8" y1="11" x2="8" y2="7" /><line x1="6" y1="9" x2="10" y2="9" />
        </svg>
      </button>
    </div>
  </div>

  {#if newItemMode}
    <div class="new-item-row">
      <span class="new-item-icon">
        {#if newItemMode === "file"}
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z" /><polyline points="10,2 10,6 14,6" /></svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z" /></svg>
        {/if}
      </span>
      <input
        bind:this={newItemInput}
        bind:value={newItemName}
        class="new-item-input"
        placeholder={newItemMode === "file" ? $t("sidebar.filenamePlaceholder") : $t("sidebar.folderPlaceholder")}
        onkeydown={handleNewKeydown}
        onblur={cancelNew}
      />
      <button class="sb-icon-btn confirm-btn" onmousedown={confirmNew} title={$t("sidebar.confirm")} data-tauri-drag-region="false">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><polyline points="4,8 7,12 13,4" /></svg>
      </button>
    </div>
    {#if newItemError}<div class="operation-error">{newItemError}</div>{/if}
  {/if}

  {#if operationError}<div class="operation-error">{operationError}</div>{/if}

  <div class="tree-scroll">
    {#if ft.loading}
      <div class="tree-empty">{$t("sidebar.loading")}</div>
    {:else if ft.error}
      <div class="tree-error">{ft.error}</div>
    {:else if ft.tree?.children?.length}
      {#each ft.tree.children as child (child.path)}
        {@render treeNode(child, 0)}
      {/each}
    {:else if ft.rootPath && ft.tree}
      <div class="tree-empty">{$t("sidebar.emptyFolder")}</div>
    {:else if !ft.rootPath}
      <div class="tree-empty hint">{$t("sidebar.openHint")}</div>
    {/if}
  </div>

  {#if $sourcesFiles.length > 0}
    <div class="section-divider"></div>
    <div class="collapsible-section">
      <button class="section-header" onclick={() => (sourcesExpanded = !sourcesExpanded)} data-tauri-drag-region="false">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" class="chevron-svg" class:rotated={sourcesExpanded}><polyline points="6,3 11,8 6,13" /></svg>
        <span class="section-label">{$t("sidebar.sources")}</span>
      </button>
      {#if sourcesExpanded}
        <div class="section-list">
          {#each $sourcesFiles as item}
            <button class="tree-row tree-file" style="padding-left: 14px" onclick={() => openInShell(item.path)} data-tauri-drag-region="false">
              <span class="tree-chevron-placeholder"></span><span class="tree-icon file-icon"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z" /><polyline points="10,2 10,6 14,6" /></svg></span>
              <span class="tree-label" title={item.path}>{item.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if $outputFiles.length > 0}
    <div class="section-divider"></div>
    <div class="collapsible-section">
      <button class="section-header" onclick={() => (outputExpanded = !outputExpanded)} data-tauri-drag-region="false">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" class="chevron-svg" class:rotated={outputExpanded}><polyline points="6,3 11,8 6,13" /></svg>
        <span class="section-label">{$t("sidebar.output")}</span>
      </button>
      {#if outputExpanded}
        <div class="section-list">
          {#each $outputFiles as item}
            <button class="tree-row tree-file" style="padding-left: 14px" onclick={() => openInShell(item.path)} data-tauri-drag-region="false">
              <span class="tree-chevron-placeholder"></span><span class="tree-icon file-icon"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z" /><polyline points="10,2 10,6 14,6" /></svg></span>
              <span class="tree-label" title={item.path}>{item.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <div class="sidebar-footer">
    <div class="segmented-btn-group">
      <button class="seg-btn" aria-label={$t("sidebar.openFolder")} onclick={handleOpenFolder} data-tauri-drag-region="false">
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v1H5" /><path d="M3 7v10a2 2 0 0 0 2 2h13.5a1.5 1.5 0 0 0 1.45-1.11L21.7 12H5.5a1.5 1.5 0 0 0-1.45 1.11L3 17" /></svg>
      </button>
      {#if outputFolderPath}
        <span class="seg-divider" aria-hidden="true"></span>
        <button class="seg-btn" aria-label={$t("sidebar.outputPanel")} title={outputFolderPath} onclick={() => openInShell(outputFolderPath)} data-tauri-drag-region="false">
          <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2" /><line x1="3" y1="14" x2="21" y2="14" /></svg>
        </button>
      {/if}
    </div>
  </div>
</div>

{#if contextMenu}
  <div class="context-menu" role="menu" style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px`}>
    <button role="menuitem" onclick={() => startRename(contextMenu!.node)}>{$t("sidebar.rename")}</button>
    <button class="danger" role="menuitem" onclick={() => deleteEntry(contextMenu!.node)}>{$t("sidebar.delete")}</button>
  </div>
{/if}

<style>
  .sidebar { display:flex; flex-direction:column; width:240px; min-width:200px; height:100%; background:var(--zc-bg-card,#FDFDFB); margin:10px 0 10px 10px; border-radius:12px; border:1px solid var(--zc-border-soft,#ECE9E2); box-shadow:0 1px 3px rgba(0,0,0,.04); overflow:hidden; flex-shrink:0; }
  .sidebar-header { display:flex; align-items:center; justify-content:space-between; padding:10px 12px 8px 14px; flex-shrink:0; }
  .sidebar-title { font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.05em; color:var(--zc-text-tertiary,#A8A49D); }
  .sidebar-actions { display:flex; gap:2px; }
  .sb-icon-btn { display:flex; align-items:center; justify-content:center; width:24px; height:24px; border:0; background:transparent; color:var(--zc-text-tertiary,#A8A49D); cursor:pointer; border-radius:4px; transition:background .1s,color .1s; }
  .sb-icon-btn:hover { background:var(--zc-active-row,#EAE6DD); color:var(--zc-text-primary,#1F1E1C); }
  .sb-icon-btn.is-pinned,.confirm-btn { color:var(--zc-text-primary,#1F1E1C); }
  .new-item-row { display:flex; align-items:center; gap:4px; padding:4px 8px 4px 14px; border-bottom:1px solid var(--zc-border-soft,#ECE9E2); background:#f9f8f5; }
  .new-item-icon,.tree-icon { display:flex; align-items:center; color:var(--zc-text-tertiary,#A8A49D); flex-shrink:0; }
  .new-item-input,.rename-input { flex:1; min-width:0; border:1px solid var(--zc-border,#E7E4DD); border-radius:3px; background:#fff; font:inherit; font-size:13px; color:var(--zc-text-primary,#1F1E1C); outline:none; padding:1px 4px; }
  .new-item-input { border:0; background:transparent; padding:2px 0; }
  .new-item-input:focus,.rename-input:focus { border-color:#9b978e; box-shadow:0 0 0 1px #9b978e; }
  .new-item-input:focus { box-shadow:none; }
  .operation-error,.tree-error { font-size:11px; color:var(--zc-danger,#C44); padding:4px 10px 4px 14px; overflow-wrap:anywhere; }
  .tree-scroll { flex:1; overflow-y:auto; padding:4px 0; }
  .tree-empty { padding:20px 14px; font-size:12px; color:var(--zc-text-tertiary,#A8A49D); }
  .tree-empty.hint { text-align:center; padding:32px 14px; }
  .tree-row { display:flex; align-items:center; width:100%; gap:2px; padding:3px 8px 3px 0; font-size:13px; color:var(--zc-text-primary,#1F1E1C); background:none; border:0; cursor:pointer; text-align:left; font-family:inherit; line-height:1.5; transition:background .08s,outline .08s; }
  .tree-row:hover { background:var(--zc-active-row,#EAE6DD); }
  .tree-row.active { background:var(--zc-active-row,#EAE6DD); font-weight:600; }
  .tree-row.selected { background:var(--zc-bg-chrome,#F4F2ED); outline:1px solid var(--zc-border,#E7E4DD); outline-offset:-1px; }
  .tree-row.drop-target { background:#e5eee2; outline:1px solid #9aae93; outline-offset:-1px; }
  .tree-row.dragging { opacity:.45; }
  .tree-folder-label { display:flex; align-items:center; gap:2px; flex:1; min-width:0; border:0; background:transparent; padding:0; cursor:pointer; font-family:inherit; font-size:inherit; color:inherit; }
  .tree-chevron,.tree-chevron-placeholder { width:20px; height:20px; flex-shrink:0; }
  .tree-chevron { display:flex; align-items:center; justify-content:center; border:0; background:transparent; color:var(--zc-text-tertiary,#A8A49D); cursor:pointer; padding:0; border-radius:3px; }
  .chevron-svg { transition:transform .15s; }
  .chevron-svg.rotated { transform:rotate(90deg); }
  .tree-icon { margin-right:4px; }
  .tree-label { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .section-divider { height:1px; background:var(--zc-border-soft,#ECE9E2); margin:0 12px; flex-shrink:0; }
  .collapsible-section { flex-shrink:0; max-height:180px; overflow-y:auto; }
  .section-header { display:flex; align-items:center; gap:4px; width:100%; padding:8px 12px 4px 14px; border:0; background:transparent; cursor:pointer; font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.05em; color:var(--zc-text-tertiary,#A8A49D); font-family:inherit; }
  .section-label { margin-left:2px; }
  .section-list { padding-bottom:4px; }
  .sidebar-footer { display:flex; flex-shrink:0; padding:8px 12px 10px; border-top:1px solid var(--zc-border-soft,#ECE9E2); }
  .segmented-btn-group { display:inline-flex; align-items:center; border:1px solid var(--zc-border,#E7E4DD); border-radius:6px; overflow:hidden; }
  .seg-btn { display:flex; align-items:center; justify-content:center; width:36px; height:32px; padding:0; border:0; background:transparent; color:var(--zc-text-secondary,#8A8782); cursor:pointer; }
  .seg-btn:hover { background:var(--zc-active-row,#EAE6DD); color:var(--zc-text-primary,#1F1E1C); }
  .seg-divider { width:1px; height:20px; background:var(--zc-border,#E7E4DD); }
  .context-menu { position:fixed; z-index:10000; width:160px; padding:5px; border:1px solid var(--zc-border,#E7E4DD); border-radius:8px; background:var(--zc-bg-card,#FDFDFB); box-shadow:0 8px 24px rgba(31,30,28,.16); }
  .context-menu button { display:block; width:100%; padding:7px 9px; border:0; border-radius:5px; background:transparent; color:var(--zc-text-primary,#1F1E1C); text-align:left; font:inherit; font-size:13px; cursor:pointer; }
  .context-menu button:hover { background:var(--zc-active-row,#EAE6DD); }
  .context-menu button.danger { color:var(--zc-danger,#C44); }
</style>
