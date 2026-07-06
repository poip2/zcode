<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { folderTree } from "$lib/stores/folderTree";
  import { openFolderDialog, listDirTree } from "$lib/tauri/files";

  let {
    open = false,
    onClose,
  }: {
    open: boolean;
    onClose: () => void;
  } = $props();

  let pinnedPath = $state<string | null>(null);
  let dialogEl: HTMLDialogElement | undefined = $state();

  let unsubPinned: () => void;

  onMount(() => {
    unsubPinned = pinnedFolder.subscribe((p) => {
      pinnedPath = p;
    });
    pinnedFolder.load();
  });

  onDestroy(() => {
    unsubPinned?.();
  });

  // Sync dialog open/close with prop
  $effect(() => {
    if (!dialogEl) return;
    if (open) {
      if (!dialogEl.open) dialogEl.showModal();
    } else {
      if (dialogEl.open) dialogEl.close();
    }
  });

  function handleDialogClose() {
    onClose();
  }

  async function handleBrowsePin() {
    const path = await openFolderDialog();
    if (path) {
      await pinnedFolder.pin(path);
      // Also auto-open it in the sidebar
      folderTree.setRoot(path);
      folderTree.setLoading(true);
      try {
        const tree = await listDirTree(path);
        folderTree.setTree(tree);
      } catch (err) {
        folderTree.setError(`Failed to read folder: ${err}`);
      }
    }
  }

  function handleClearPin() {
    pinnedFolder.unpin();
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) {
      onClose();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialogEl}
  class="settings-dialog"
  onclick={handleBackdropClick}
  onclose={handleDialogClose}
>
  <div
    class="dialog-panel"
    role="presentation"
    onclick={(e) => e.stopPropagation()}
    onkeydown={() => {}}
  >
    <div class="dialog-header">
      <span class="dialog-title">Settings</span>
      <button
        class="dialog-close-btn"
        onclick={onClose}
        title="Close"
        data-tauri-drag-region="false"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <line x1="4" y1="4" x2="12" y2="12"/>
          <line x1="12" y1="4" x2="4" y2="12"/>
        </svg>
      </button>
    </div>

    <div class="dialog-body">
      <!-- Default pin folder -->
      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Default pin folder</span>
          <span class="setting-desc">
            {#if pinnedPath}
              <code class="pin-path">{pinnedPath}</code>
            {:else}
              No folder pinned
            {/if}
          </span>
        </div>
        <div class="setting-actions">
          <button
            class="setting-btn"
            onclick={handleBrowsePin}
            data-tauri-drag-region="false"
          >
            {pinnedPath ? "Change…" : "Browse…"}
          </button>
          {#if pinnedPath}
            <button
              class="setting-btn setting-btn-clear"
              onclick={handleClearPin}
              data-tauri-drag-region="false"
            >
              Clear
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
</dialog>

<style>
  .settings-dialog {
    border: none;
    border-radius: 12px;
    padding: 0;
    background: var(--zc-bg-card, #FDFDFB);
    box-shadow: 0 8px 30px rgba(0,0,0,0.12);
    min-width: 360px;
    max-width: 440px;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .settings-dialog::backdrop {
    background: rgba(0,0,0,0.15);
  }

  .dialog-panel {
    padding: 0;
  }

  .dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
  }

  .dialog-title {
    font-size: 14px;
    font-weight: 600;
  }

  .dialog-close-btn {
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
  }

  .dialog-close-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
  }

  .dialog-body {
    padding: 16px;
  }

  .setting-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .setting-info {
    flex: 1;
    min-width: 0;
  }

  .setting-label {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--zc-text-primary, #1F1E1C);
    margin-bottom: 4px;
  }

  .setting-desc {
    display: block;
    font-size: 12px;
    color: var(--zc-text-secondary, #8A8782);
    word-break: break-all;
    line-height: 1.4;
  }

  .pin-path {
    font-family: "SF Mono", "JetBrains Mono", Menlo, monospace;
    font-size: 11px;
    background: #f2f2f7;
    padding: 1px 4px;
    border-radius: 4px;
    word-break: break-all;
  }

  .setting-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .setting-btn {
    padding: 5px 12px;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    background: var(--zc-text-primary, #1F1E1C);
    color: #fff;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }

  .setting-btn:hover {
    opacity: 0.88;
  }

  .setting-btn-clear {
    background: transparent;
    color: var(--zc-text-secondary, #8A8782);
    border: 1px solid var(--zc-border, #E7E4DD);
  }

  .setting-btn-clear:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
    opacity: 1;
  }
</style>
