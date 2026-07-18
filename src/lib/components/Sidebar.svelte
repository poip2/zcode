<script module lang="ts">
  let autoLoadDone = false;
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { document as docStore } from "$lib/stores/document";
  import { folderTree, type DirNode } from "$lib/stores/folderTree";
  import { recents } from "$lib/stores/recents";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import {
    loadFile,
    openFolderDialog,
    listDirTree,
    createMarkdownFile,
    createFolder,
    pathExists,
    openInShell,
    getPortableDataDir,
    joinPath,
  } from "$lib/tauri/files";
  import { load as loadSettings } from "$lib/stores/settings";

  // New file/folder inline editing
  let newItemMode = $state<null | "file" | "folder">(null);
  let newItemName = $state("");
  let newItemError = $state("");
  let newItemInput: HTMLInputElement | undefined = $state();

  let recentExpanded = $state(true);
  let outputFolderPath = $state<string>("");

  let doc = $derived($docStore);
  let ft = $derived($folderTree);
  let expanded = $derived(folderTree.expanded);
  let pinnedPath = $derived($pinnedFolder);

  let recentList = $state<{ path: string; name: string; openedAt: number }[]>([]);
  const unsubRecents = recents.subscribe((list) => {
    recentList = list;
  });

  onDestroy(() => {
    unsubRecents();
  });

  onMount(async () => {
    recents.load();
    await pinnedFolder.load();

    // Resolve output folder path
    getPortableDataDir().then((dataDir) => {
      loadSettings().then(async (s) => {
        outputFolderPath = s.outputFolder || await joinPath(dataDir, "output");
      });
    });
    if (!autoLoadDone) {
      autoLoadDone = true;
      const p = $pinnedFolder;
      if (p) {
        const exists = await pathExists(p).catch(() => false);
        if (exists) {
          const current = get(folderTree);
          if (current.rootPath === p && current.tree !== null) return;
          folderTree.setRoot(p);
          folderTree.setLoading(true);
          try {
            const tree = await listDirTree(p);
            folderTree.setTree(tree);
          } catch (err) {
            folderTree.setError(`Failed to read pinned folder: ${err}`);
          }
        }
      }
    }
  });

  function startNew(mode: "file" | "folder") {
    newItemMode = mode;
    newItemName = "";
    newItemError = "";
    // Focus after DOM update
    requestAnimationFrame(() => {
      newItemInput?.focus();
    });
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

    const dir = ft.rootPath;
    if (!dir) return;

    try {
      if (newItemMode === "file") {
        const createdPath = await createMarkdownFile(dir, name);
        await refreshTree();
        await loadFile(createdPath);
        cancelNew();
      } else {
        await createFolder(dir, name);
        await refreshTree();
        cancelNew();
      }
    } catch (err) {
      newItemError = String(err);
    }
  }

  function handleNewKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      confirmNew();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelNew();
    }
  }

  async function handleOpenFolder() {
    const path = await openFolderDialog();
    if (!path) return;
    await openFolderPath(path);
  }

  async function openFolderPath(path: string) {
    folderTree.setRoot(path);
    folderTree.setLoading(true);
    try {
      const tree = await listDirTree(path);
      folderTree.setTree(tree);
    } catch (err) {
      folderTree.setError(`Failed to read folder: ${err}`);
    }
  }

  function handlePin() {
    const path = ft.rootPath;
    if (!path) return;
    if (pinnedPath === path) {
      pinnedFolder.unpin();
    } else {
      pinnedFolder.pin(path);
    }
  }

  async function refreshTree() {
    if (!ft.rootPath) return;
    try {
      const tree = await listDirTree(ft.rootPath);
      folderTree.setTree(tree);
    } catch (err) {
      folderTree.setError(`Failed to read folder: ${err}`);
    }
  }

  function handleFileClick(node: DirNode) {
    loadFile(node.path);
  }

  function toggleDir(path: string) {
    folderTree.expanded.toggle(path);
  }
</script>

<div class="sidebar">
  <!-- Header -->
  <div class="sidebar-header">
    <span class="sidebar-title">Files</span>
    <div class="sidebar-actions">
      <!-- Pin / unpin current folder -->
      {#if ft.rootPath}
        <button
          class="sb-icon-btn"
          class:is-pinned={pinnedPath === ft.rootPath}
          title={pinnedPath === ft.rootPath ? "Unpin folder" : "Pin folder"}
          onclick={handlePin}
          data-tauri-drag-region="false"
        >
          {#if pinnedPath === ft.rootPath}
            <!-- filled pin -->
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" stroke="none">
              <path d="M6.5 1.5a.5.5 0 0 1 .5.5v4l2.5 2.5V14l-2-2-2 2V8.5L8 6V2a.5.5 0 0 1 .5-.5h-2z"/>
              <path d="M9.5 1.5L5.5 12" stroke="currentColor" stroke-width="1.5" fill="none"/>
            </svg>
          {:else}
            <!-- outline pin -->
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
              <path d="M6.5 1.5v4L9 8v6l-2-2-2 2V8l2.5-2.5v-4"/>
              <line x1="5" y1="1.5" x2="10" y2="1.5"/>
            </svg>
          {/if}
        </button>
      {/if}
      <!-- New file -->
      <button
        class="sb-icon-btn"
        title="New file"
        onclick={() => startNew("file")}
        data-tauri-drag-region="false"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
          <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
          <line x1="8" y1="11" x2="8" y2="7"/>
          <line x1="6" y1="9" x2="10" y2="9"/>
        </svg>
      </button>
      <!-- New folder -->
      <button
        class="sb-icon-btn"
        title="New folder"
        onclick={() => startNew("folder")}
        data-tauri-drag-region="false"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
          <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
          <line x1="8" y1="11" x2="8" y2="7"/>
          <line x1="6" y1="9" x2="10" y2="9"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- New item inline input -->
  {#if newItemMode}
    <div class="new-item-row">
      <span class="new-item-icon">
        {#if newItemMode === "file"}
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
            <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
            <polyline points="10,2 10,6 14,6"/>
          </svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
            <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
          </svg>
        {/if}
      </span>
      <input
        bind:this={newItemInput}
        bind:value={newItemName}
        class="new-item-input"
        placeholder={newItemMode === "file" ? "filename.md" : "folder name"}
        onkeydown={handleNewKeydown}
        onblur={cancelNew}
      />
      <button class="sb-icon-btn confirm-btn" onmousedown={confirmNew} title="Confirm" data-tauri-drag-region="false">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <polyline points="4,8 7,12 13,4"/>
        </svg>
      </button>
    </div>
    {#if newItemError}
      <div class="new-item-error">{newItemError}</div>
    {/if}
  {/if}

  <!-- File tree -->
  <div class="tree-scroll">
    {#if ft.loading}
      <div class="tree-empty">Loading…</div>
    {:else if ft.error}
      <div class="tree-error">{ft.error}</div>
    {:else if ft.tree?.children?.length}
      {#each ft.tree.children as child}
        {@const key = child.path}
        {#if child.is_dir}
          {@const open = $expanded.has(key)}
          {@const hasKids = child.children && child.children.length > 0}
          <div class="tree-row depth-0">
            {#if hasKids}
              <button
                class="tree-chevron"
                onclick={() => toggleDir(key)}
                aria-expanded={open}
                aria-label={open ? "Collapse folder" : "Expand folder"}
                data-tauri-drag-region="false"
              >
                <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" class="chevron-svg" class:rotated={open}>
                  <polyline points="6,3 11,8 6,13"/>
                </svg>
              </button>
            {:else}
              <span class="tree-chevron-placeholder"></span>
            {/if}
            <span class="tree-icon">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
              </svg>
            </span>
            <span class="tree-label">{child.name}</span>
          </div>
          {#if hasKids && open}
            {#each child.children as sub}
              {#if sub.is_dir}
                {@const subKey = sub.path}
                {@const subOpen = $expanded.has(subKey)}
                {@const subHasKids = sub.children && sub.children.length > 0}
                <div class="tree-row depth-1">
                  {#if subHasKids}
                    <button
                      class="tree-chevron"
                      onclick={() => toggleDir(subKey)}
                      aria-expanded={subOpen}
                      aria-label={subOpen ? "Collapse folder" : "Expand folder"}
                      data-tauri-drag-region="false"
                    >
                      <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" class="chevron-svg" class:rotated={subOpen}>
                        <polyline points="6,3 11,8 6,13"/>
                      </svg>
                    </button>
                  {:else}
                    <span class="tree-chevron-placeholder"></span>
                  {/if}
                  <span class="tree-icon">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                      <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
                    </svg>
                  </span>
                  <span class="tree-label">{sub.name}</span>
                </div>
                {#if subHasKids && subOpen}
                  {#each sub.children as leaf}
                    {#if leaf.is_dir}
                      {@const leafKey = leaf.path}
                      {@const leafOpen = $expanded.has(leafKey)}
                      {@const leafHasKids = leaf.children && leaf.children.length > 0}
                      <div class="tree-row depth-2">
                        {#if leafHasKids}
                          <button
                            class="tree-chevron"
                            onclick={() => toggleDir(leafKey)}
                            aria-expanded={leafOpen}
                            aria-label={leafOpen ? "Collapse folder" : "Expand folder"}
                            data-tauri-drag-region="false"
                          >
                            <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" class="chevron-svg" class:rotated={leafOpen}>
                              <polyline points="6,3 11,8 6,13"/>
                            </svg>
                          </button>
                        {:else}
                          <span class="tree-chevron-placeholder"></span>
                        {/if}
                        <span class="tree-icon">
                          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                            <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
                          </svg>
                        </span>
                        <span class="tree-label">{leaf.name}</span>
                      </div>
                      {#if leafHasKids && leafOpen}
                        {#each leaf.children as deep}
                          {#if deep.is_dir}
                            <!-- depth-3 empty dir (max depth reached) -->
                            <div class="tree-row depth-3">
                              <span class="tree-chevron-placeholder"></span>
                              <span class="tree-icon">
                                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                                  <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
                                </svg>
                              </span>
                              <span class="tree-label">{deep.name}</span>
                            </div>
                          {:else}
                            <!-- depth-3 file -->
                            <button
                              class="tree-row tree-file depth-3"
                              class:active={doc.filePath === deep.path}
                              onclick={() => handleFileClick(deep)}
                              data-tauri-drag-region="false"
                            >
                              <span class="tree-icon file-icon">
                                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                                  <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
                                  <polyline points="10,2 10,6 14,6"/>
                                </svg>
                              </span>
                              <span class="tree-label">{deep.name}</span>
                            </button>
                          {/if}
                        {/each}
                      {/if}
                    {:else}
                      <!-- depth-2 file -->
                      <button
                        class="tree-row tree-file depth-2"
                        class:active={doc.filePath === leaf.path}
                        onclick={() => handleFileClick(leaf)}
                        data-tauri-drag-region="false"
                      >
                        <span class="tree-icon file-icon">
                          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                            <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
                            <polyline points="10,2 10,6 14,6"/>
                          </svg>
                        </span>
                        <span class="tree-label">{leaf.name}</span>
                      </button>
                    {/if}
                  {/each}
                {/if}
              {:else}
                <!-- depth-1 file -->
                <button
                  class="tree-row tree-file depth-1"
                  class:active={doc.filePath === sub.path}
                  onclick={() => handleFileClick(sub)}
                  data-tauri-drag-region="false"
                >
                  <span class="tree-icon file-icon">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                      <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
                      <polyline points="10,2 10,6 14,6"/>
                    </svg>
                  </span>
                  <span class="tree-label">{sub.name}</span>
                </button>
              {/if}
            {/each}
          {/if}
        {:else}
          <!-- depth-0 file -->
          <button
            class="tree-row tree-file depth-0"
            class:active={doc.filePath === child.path}
            onclick={() => handleFileClick(child)}
            data-tauri-drag-region="false"
          >
            <span class="tree-icon file-icon">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
                <polyline points="10,2 10,6 14,6"/>
              </svg>
            </span>
            <span class="tree-label">{child.name}</span>
          </button>
        {/if}
      {/each}
    {:else if ft.rootPath && ft.tree}
      <div class="tree-empty">Empty folder</div>
    {:else if !ft.rootPath}
      <div class="tree-empty hint">Open a folder to browse files</div>
    {/if}
  </div>

  <!-- Divider + Recent section -->
  {#if recentList.length > 0}
    <div class="section-divider"></div>
    <div class="recent-section">
      <button
        class="recent-header"
        onclick={() => (recentExpanded = !recentExpanded)}
        data-tauri-drag-region="false"
      >
        <svg
          width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor"
          stroke-width="1.5" class="chevron-svg" class:rotated={recentExpanded}
        >
          <polyline points="6,3 11,8 6,13"/>
        </svg>
        <span class="recent-label">Recent</span>
      </button>
      {#if recentExpanded}
        <div class="recent-list">
          {#each recentList as item}
            <button
              class="tree-row tree-file depth-0"
              class:active={doc.filePath === item.path}
              onclick={() => loadFile(item.path)}
              data-tauri-drag-region="false"
            >
              <span class="tree-icon file-icon">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
                  <path d="M10 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6z"/>
                  <polyline points="10,2 10,6 14,6"/>
                </svg>
              </span>
              <span class="tree-label" title={item.path}>{item.name}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Segmented icon button group -->
  <div class="sidebar-footer">
    <div class="segmented-btn-group">
      <button
        class="seg-btn"
        aria-label="打开文件夹"
        onclick={handleOpenFolder}
        data-tauri-drag-region="false"
      >
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 7a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v1H5" />
          <path d="M3 7v10a2 2 0 0 0 2 2h13.5a1.5 1.5 0 0 0 1.45-1.11L21.7 12H5.5a1.5 1.5 0 0 0-1.45 1.11L3 17"/>
        </svg>
      </button>
      {#if outputFolderPath}
        <span class="seg-divider" aria-hidden="true"></span>
        <button
          class="seg-btn"
          aria-label="输出面板"
          title={outputFolderPath}
          onclick={() => openInShell(outputFolderPath)}
          data-tauri-drag-region="false"
        >
          <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
            <rect x="3" y="4" width="18" height="16" rx="2"/>
            <line x1="3" y1="14" x2="21" y2="14"/>
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 240px;
    min-width: 200px;
    height: 100%;
    background: var(--zc-bg-card, #FDFDFB);
    margin: 10px 0 10px 10px;
    border-radius: 12px;
    border: 1px solid var(--zc-border-soft, #ECE9E2);
    box-shadow: 0 1px 3px rgba(0,0,0,0.04);
    overflow: hidden;
    flex-shrink: 0;
  }

  /* Header */
  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 8px 14px;
    flex-shrink: 0;
  }

  .sidebar-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .sidebar-actions {
    display: flex;
    gap: 2px;
  }

  .sb-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: transparent;
    color: var(--zc-text-tertiary, #A8A49D);
    cursor: pointer;
    border-radius: 4px;
    transition: background 0.1s, color 0.1s;
  }

  .sb-icon-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
  }

  .sb-icon-btn.is-pinned {
    color: var(--zc-text-primary, #1F1E1C);
  }

  .confirm-btn {
    color: var(--zc-text-primary, #1F1E1C);
  }

  .confirm-btn:hover {
    background: #e0dcd2;
  }

  /* New item inline row */
  .new-item-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px 4px 14px;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
    background: #f9f8f5;
  }

  .new-item-icon {
    display: flex;
    align-items: center;
    color: var(--zc-text-tertiary, #A8A49D);
    flex-shrink: 0;
  }

  .new-item-input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: 13px;
    font-family: inherit;
    color: var(--zc-text-primary, #1F1E1C);
    outline: none;
    padding: 2px 0;
  }

  .new-item-input::placeholder {
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .new-item-error {
    font-size: 11px;
    color: var(--zc-danger, #C44);
    padding: 2px 8px 2px 14px;
    display: block;
    width: 100%;
  }

  /* Tree scroll area */
  .tree-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .tree-empty {
    padding: 20px 14px;
    font-size: 12px;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .tree-empty.hint {
    text-align: center;
    padding: 32px 14px;
  }

  .tree-error {
    padding: 12px 14px;
    font-size: 12px;
    color: #e67e22;
  }

  /* Tree rows */
  .tree-row {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 2px;
    padding: 3px 8px 3px 0;
    font-size: 13px;
    color: var(--zc-text-primary, #1F1E1C);
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    line-height: 1.5;
    transition: background 0.08s;
  }

  .tree-row:hover {
    background: var(--zc-active-row, #EAE6DD);
  }

  .tree-row.active {
    background: var(--zc-active-row, #EAE6DD);
    font-weight: 600;
  }

  .tree-file {
    cursor: pointer;
  }

  .depth-0 { padding-left: 14px; }
  .depth-1 { padding-left: 30px; }
  .depth-2 { padding-left: 46px; }
  .depth-3 { padding-left: 62px; }

  .tree-chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    border: none;
    background: transparent;
    color: var(--zc-text-tertiary, #A8A49D);
    cursor: pointer;
    padding: 0;
    border-radius: 3px;
    transition: color 0.1s;
  }

  .tree-chevron:hover {
    color: var(--zc-text-primary, #1F1E1C);
  }

  .tree-chevron-placeholder {
    display: inline-block;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
  }

  .chevron-svg {
    transition: transform 0.15s;
  }

  .chevron-svg.rotated {
    transform: rotate(90deg);
  }

  .tree-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--zc-text-tertiary, #A8A49D);
    margin-right: 4px;
  }

  .file-icon {
    margin-left: 20px;
  }

  .tree-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Section divider */
  .section-divider {
    height: 1px;
    background: var(--zc-border-soft, #ECE9E2);
    margin: 0 12px;
    flex-shrink: 0;
  }

  /* Recent section */
  .recent-section {
    flex-shrink: 0;
    max-height: 180px;
    overflow-y: auto;
  }

  .recent-header {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 8px 12px 4px 14px;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--zc-text-tertiary, #A8A49D);
    font-family: inherit;
  }

  .recent-header:hover {
    color: var(--zc-text-secondary, #8A8782);
  }

  .recent-label {
    margin-left: 2px;
  }

  .recent-list {
    padding-bottom: 4px;
  }

  /* Footer */
  .sidebar-footer {
    display: flex;
    flex-shrink: 0;
    padding: 8px 12px 10px;
    border-top: 1px solid var(--zc-border-soft, #ECE9E2);
  }

  /* Segmented icon button group */
  .segmented-btn-group {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--zc-border, #E7E4DD);
    border-radius: 6px;
    overflow: hidden;
  }

  .seg-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 32px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--zc-text-secondary, #8A8782);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }

  .seg-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
  }

  .seg-divider {
    display: block;
    width: 1px;
    height: 20px;
    background: var(--zc-border, #E7E4DD);
    flex-shrink: 0;
  }
</style>
