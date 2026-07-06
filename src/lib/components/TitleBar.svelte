<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { document as docStore } from "$lib/stores/document";

  let {
    sidebarVisible,
    onToggleSidebar,
    onOpenSettings,
  }: {
    sidebarVisible: boolean;
    onToggleSidebar: () => void;
    onOpenSettings: () => void;
  } = $props();

  let doc = $derived($docStore);

  // Window controls
  function minimize() {
    getCurrentWindow().minimize();
  }

  function toggleMaximize() {
    getCurrentWindow().toggleMaximize();
  }

  function closeWindow() {
    getCurrentWindow().close();
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <!-- Left: sidebar toggle -->
  <button
    class="tb-btn tb-toggle"
    onclick={onToggleSidebar}
    title={sidebarVisible ? "Hide sidebar" : "Show sidebar"}
    data-tauri-drag-region="false"
  >
    {#if sidebarVisible}
      <!-- Panel left open icon -->
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
        <rect x="1" y="2" width="5" height="12" rx="1"/>
        <rect x="8" y="2" width="7" height="12" rx="1"/>
        <line x1="10" y1="6" x2="13" y2="6"/>
        <line x1="10" y1="10" x2="13" y2="10"/>
      </svg>
    {:else}
      <!-- Panel right open icon -->
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="2" width="12" height="12" rx="1"/>
        <line x1="6" y1="2" x2="6" y2="14"/>
        <line x1="10" y1="6" x2="13" y2="6"/>
        <line x1="10" y1="10" x2="13" y2="10"/>
      </svg>
    {/if}
  </button>

  <!-- Settings -->
  <button
    class="tb-btn"
    onclick={onOpenSettings}
    title="Settings"
    data-tauri-drag-region="false"
  >
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="8" cy="8" r="2.5"/>
      <path d="M8 1.5v1.5M8 13v1.5M3.4 3.4l1.06 1.06M11.54 11.54l1.06 1.06M1.5 8H3M13 8h1.5M3.4 12.6l1.06-1.06M11.54 4.46l1.06-1.06"/>
    </svg>
  </button>

  <!-- Center: filename + dropdown (TODO) -->
  <div class="tb-center">
    <span class="tb-filename">{doc.fileName ?? "zcode"}</span>
    <!-- TODO: dropdown for file switcher -->
  </div>

  <!-- Right: window controls -->
  <div class="tb-controls" data-tauri-drag-region="false">
    <button class="tb-btn" onclick={minimize} title="Minimize">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3">
        <line x1="3" y1="8" x2="13" y2="8"/>
      </svg>
    </button>
    <button class="tb-btn" onclick={toggleMaximize} title="Maximize">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3">
        <rect x="3" y="3" width="10" height="10" rx="1"/>
      </svg>
    </button>
    <button class="tb-btn tb-close" onclick={closeWindow} title="Close">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <line x1="4" y1="4" x2="12" y2="12"/>
        <line x1="12" y1="4" x2="4" y2="12"/>
      </svg>
    </button>
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    height: 36px;
    background: var(--zc-bg-chrome, #F4F2ED);
    border-bottom: 1px solid var(--zc-border, #E7E4DD);
    user-select: none;
    flex-shrink: 0;
  }

  .tb-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: none;
    background: transparent;
    color: var(--zc-text-secondary, #8A8782);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
    -webkit-app-region: no-drag;
  }

  .tb-btn:hover {
    background: rgba(0,0,0,0.05);
    color: var(--zc-text-primary, #1F1E1C);
  }

  .tb-toggle {
    margin-left: 2px;
  }

  .tb-close:hover {
    background: #e81123;
    color: white;
  }

  .tb-center {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 4px;
    min-width: 0;
  }

  .tb-filename {
    font-size: 12px;
    font-weight: 500;
    color: var(--zc-text-secondary, #8A8782);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 400px;
  }

  .tb-controls {
    display: flex;
    margin-left: auto;
  }
</style>
