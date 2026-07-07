<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { folderTree } from "$lib/stores/folderTree";
  import { openFolderDialog, listDirTree } from "$lib/tauri/files";
  import { load as loadSettings, save as saveSettings, type SkillsSettings } from "$lib/stores/settings";

  let {
    open = false,
    onClose,
  }: {
    open: boolean;
    onClose: () => void;
  } = $props();

  // ── Persisted settings (loaded once on mount, updated on Save) ──
  let persistedAiBaseUrl = $state("");
  let persistedAiApiKey = $state("");
  let persistedAiModel = $state("");
  let persistedSkills: SkillsSettings = $state({
    summarize: true,
    fixGrammar: true,
    generateToc: false,
    explainCode: false,
  });

  // ── Tab state ──
  let activeTab = $state<"folder" | "ai" | "skills">("folder");
  let pinnedPath = $state<string | null>(null);

  // ── Draft state (populated from persisted on each open, written back on Save) ──
  let draftBaseUrl = $state("");
  let draftApiKey = $state("");
  let draftModel = $state("");
  let showApiKey = $state(false);

  let draftSkills = $state([
    { name: "Summarize document", desc: "Generate a short summary of the open file", key: "summarize" as const, enabled: true },
    { name: "Fix grammar", desc: "Rewrite the selection with corrected grammar", key: "fixGrammar" as const, enabled: true },
    { name: "Generate table of contents", desc: "Insert a TOC from the document's headings", key: "generateToc" as const, enabled: false },
    { name: "Explain code block", desc: "Add an explanation above the selected code fence", key: "explainCode" as const, enabled: false },
  ]);

  let dialogEl: HTMLDialogElement | undefined = $state();
  let unsubPinned: () => void;

  onMount(async () => {
    unsubPinned = pinnedFolder.subscribe((p) => {
      pinnedPath = p;
    });
    pinnedFolder.load();

    // Load persisted settings from disk (once at startup)
    const s = await loadSettings();
    persistedAiBaseUrl = s.aiProvider.baseUrl;
    persistedAiApiKey = s.aiProvider.apiKey;
    persistedAiModel = s.aiProvider.model;
    persistedSkills = { ...s.skills };
  });

  onDestroy(() => {
    unsubPinned?.();
  });

  // Sync dialog open/close with prop, and populate draft on each open
  $effect(() => {
    if (!dialogEl) return;
    if (open) {
      // Populate draft from persisted values on each open.
      // IMPORTANT: do NOT read draftSkills / draftBaseUrl etc. inside this
      // effect — writing to them is fine, but reading them would make the
      // effect depend on them and cause an infinite loop.
      draftBaseUrl = persistedAiBaseUrl;
      draftApiKey = persistedAiApiKey;
      draftModel = persistedAiModel;
      showApiKey = false;
      // Rebuild array without reading draftSkills (avoids infinite loop)
      draftSkills = [
        { name: "Summarize document", desc: "Generate a short summary of the open file", key: "summarize" as const, enabled: persistedSkills.summarize },
        { name: "Fix grammar", desc: "Rewrite the selection with corrected grammar", key: "fixGrammar" as const, enabled: persistedSkills.fixGrammar },
        { name: "Generate table of contents", desc: "Insert a TOC from the document's headings", key: "generateToc" as const, enabled: persistedSkills.generateToc },
        { name: "Explain code block", desc: "Add an explanation above the selected code fence", key: "explainCode" as const, enabled: persistedSkills.explainCode },
      ];
      if (!dialogEl.open) dialogEl.showModal();
    } else {
      if (dialogEl.open) dialogEl.close();
    }
  });

  function handleDialogClose() {
    onClose();
  }

  /** Cancel or backdrop click — close without saving */
  function handleCancel() {
    onClose();
  }

  /** Save draft to disk, then close */
  async function handleSave() {
    // Write draft to persisted state
    persistedAiBaseUrl = draftBaseUrl;
    persistedAiApiKey = draftApiKey;
    persistedAiModel = draftModel;
    const skillsSummary: SkillsSettings = {
      summarize: false,
      fixGrammar: false,
      generateToc: false,
      explainCode: false,
    };
    for (const s of draftSkills) {
      skillsSummary[s.key] = s.enabled;
    }
    persistedSkills = skillsSummary;

    const ok = await saveSettings({
      aiProvider: {
        baseUrl: draftBaseUrl,
        apiKey: draftApiKey,
        model: draftModel,
      },
      skills: skillsSummary,
    });

    if (ok) onClose();
  }

  async function handleBrowsePin() {
    const path = await openFolderDialog();
    if (path) {
      await pinnedFolder.pin(path);
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

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) {
      handleCancel();
    }
  }

  function toggleSkill(index: number) {
    draftSkills = draftSkills.map((s, i) =>
      i === index ? { ...s, enabled: !s.enabled } : s
    );
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
    <!-- Header -->
    <div class="settings-header">
      <h2>Settings</h2>
      <button
        class="settings-close"
        onclick={handleCancel}
        title="Close"
        data-tauri-drag-region="false"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <line x1="4" y1="4" x2="12" y2="12"/>
          <line x1="12" y1="4" x2="4" y2="12"/>
        </svg>
      </button>
    </div>

    <!-- Tabs -->
    <div class="settings-tabs">
      <button
        class="settings-tab"
        class:active={activeTab === "folder"}
        onclick={() => (activeTab = "folder")}
      >Default Folder</button>
      <button
        class="settings-tab"
        class:active={activeTab === "ai"}
        onclick={() => (activeTab = "ai")}
      >AI Provider</button>
      <button
        class="settings-tab"
        class:active={activeTab === "skills"}
        onclick={() => (activeTab = "skills")}
      >Skills</button>
    </div>

    <!-- Body -->
    <div class="settings-body">

      <!-- Section: Default Folder -->
      {#if activeTab === "folder"}
        <section class="settings-section">
          <div class="settings-section-title">Default Folder</div>
          <p class="settings-section-desc">The folder zcode opens automatically on launch.</p>
          <div class="folder-field">
            <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3">
              <path d="M2 4a1 1 0 0 1 1-1h3l2 2h5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>
            </svg>
            <span class="folder-path" title={pinnedPath ?? ""}>{pinnedPath ?? "No folder pinned"}</span>
            <button
              class="settings-btn-secondary"
              onclick={handleBrowsePin}
              data-tauri-drag-region="false"
            >{pinnedPath ? "Change…" : "Browse…"}</button>
          </div>
        </section>
      {/if}

      <!-- Section: AI Provider -->
      {#if activeTab === "ai"}
        <section class="settings-section">
          <div class="settings-section-title">AI Provider</div>
          <p class="settings-section-desc">Connect zcode to an OpenAI-compatible endpoint.</p>

          <label class="settings-label">Base URL</label>
          <input
            class="settings-input mono"
            type="text"
            placeholder="https://api.openai.com/v1"
            bind:value={draftBaseUrl}
          />

          <label class="settings-label">API Key</label>
          <div class="api-key-field">
            <input
              class="settings-input mono"
              type={showApiKey ? "text" : "password"}
              bind:value={draftApiKey}
            />
            <button
              class="icon-toggle-btn"
              title="Show/hide key"
              onclick={() => (showApiKey = !showApiKey)}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7-11-7-11-7z"/>
                <circle cx="12" cy="12" r="3"/>
              </svg>
            </button>
          </div>

          <label class="settings-label">Model</label>
          <input
            class="settings-input mono"
            type="text"
            placeholder="gpt-4o"
            bind:value={draftModel}
          />
        </section>
      {/if}

      <!-- Section: Skills -->
      {#if activeTab === "skills"}
        <section class="settings-section">
          <div class="settings-section-title">Skills</div>
          <p class="settings-section-desc">AI-assisted actions available from the command palette.</p>

          {#each draftSkills as skill, i (skill.key)}
            <div class="skill-row">
              <div class="skill-info">
                <span class="skill-name">{skill.name}</span>
                <span class="skill-desc">{skill.desc}</span>
              </div>
              <label class="switch">
                <input
                  type="checkbox"
                  checked={skill.enabled}
                  onchange={() => toggleSkill(i)}
                />
                <span class="switch-slider"></span>
              </label>
            </div>
          {/each}

          <button class="settings-btn-secondary settings-add-skill">+ Add custom skill</button>
        </section>
      {/if}
    </div>

    <!-- Footer -->
    <div class="settings-footer">
      <button class="settings-btn-secondary" onclick={handleCancel}>Cancel</button>
      <button class="settings-btn-primary" onclick={handleSave}>Save</button>
    </div>
  </div>
</dialog>

<style>
  .settings-dialog {
    border: none;
    border-radius: 12px;
    padding: 0;
    margin: auto;
    background: var(--zc-bg-card, #FDFDFB);
    box-shadow: 0 16px 44px rgba(0,0,0,0.24);
    min-width: 360px;
    max-width: 440px;
    width: 100%;
    max-height: 90vh;
    color: var(--zc-text-primary, #1F1E1C);
    overflow: hidden;
  }

  .settings-dialog::backdrop {
    background: rgba(24, 21, 16, 0.32);
  }

  .dialog-panel {
    display: flex;
    flex-direction: column;
    max-height: 90vh;
  }

  /* ── Header ── */
  .settings-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 13px 14px 11px 16px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
  }

  .settings-header h2 {
    font-size: 14px;
    font-weight: 700;
    margin-right: auto;
  }

  .settings-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: transparent;
    color: var(--zc-text-secondary, #8A8782);
    cursor: pointer;
    border-radius: 6px;
    flex-shrink: 0;
  }

  .settings-close:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
  }

  /* ── Tabs ── */
  .settings-tabs {
    display: flex;
    gap: 4px;
    padding: 8px 14px 0;
    flex-shrink: 0;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
  }

  .settings-tab {
    border: none;
    background: transparent;
    font-size: 12px;
    font-weight: 500;
    padding: 7px 4px;
    cursor: pointer;
    color: var(--zc-text-secondary, #8A8782);
    font-family: inherit;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color 0.15s;
  }

  .settings-tab.active {
    color: var(--zc-text-primary, #1F1E1C);
    border-bottom-color: var(--zc-text-primary, #1F1E1C);
  }

  /* ── Body ── */
  .settings-body {
    padding: 16px;
    overflow-y: auto;
    flex: 1;
  }

  .settings-section-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--zc-text-tertiary, #A8A49D);
    margin-bottom: 4px;
  }

  .settings-section-desc {
    font-size: 12px;
    color: var(--zc-text-secondary, #8A8782);
    margin-bottom: 12px;
    line-height: 1.5;
  }

  .settings-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--zc-text-secondary, #8A8782);
    margin: 12px 0 5px;
  }

  .settings-label:first-of-type {
    margin-top: 0;
  }

  .settings-input {
    width: 100%;
    padding: 7px 10px;
    font-size: 13px;
    font-family: inherit;
    border: 1px solid #D9D5CC;
    border-radius: 6px;
    background: #fff;
    color: var(--zc-text-primary, #1F1E1C);
    outline: none;
    box-sizing: border-box;
  }

  .settings-input:focus {
    border-color: #B9B6B0;
  }

  .settings-input.mono {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12.5px;
  }

  /* ── Default Folder field ── */
  .folder-field {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--zc-bg-chrome, #F4F2ED);
    border: 1px solid var(--zc-border-soft, #ECE9E2);
    border-radius: 8px;
  }

  .folder-field svg {
    flex-shrink: 0;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .folder-path {
    flex: 1;
    font-size: 12.5px;
    font-family: "SF Mono", Menlo, monospace;
    color: var(--zc-text-primary, #1F1E1C);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── API key field ── */
  .api-key-field {
    position: relative;
  }

  .api-key-field .settings-input {
    padding-right: 34px;
  }

  .icon-toggle-btn {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--zc-text-tertiary, #A8A49D);
    cursor: pointer;
    border-radius: 5px;
  }

  .icon-toggle-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
  }

  /* ── Buttons ── */
  .settings-btn-secondary {
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    border: 1px solid #D9D5CC;
    background: #fff;
    border-radius: 6px;
    cursor: pointer;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .settings-btn-secondary:hover {
    background: var(--zc-bg-chrome, #F4F2ED);
  }

  .settings-btn-primary {
    padding: 6px 14px;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    border: none;
    background: var(--zc-text-primary, #1F1E1C);
    color: #fff;
    border-radius: 6px;
    cursor: pointer;
  }

  .settings-btn-primary:hover {
    opacity: 0.88;
  }

  .settings-add-skill {
    margin-top: 8px;
    width: 100%;
  }

  /* ── Skills ── */
  .skill-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 0;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
  }

  .skill-row:last-of-type {
    border-bottom: none;
  }

  .skill-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .skill-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .skill-desc {
    font-size: 11.5px;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  /* ── Toggle switch ── */
  .switch {
    position: relative;
    display: inline-block;
    width: 34px;
    height: 20px;
    flex-shrink: 0;
    margin-left: auto;
  }

  .switch input {
    opacity: 0;
    width: 0;
    height: 0;
    position: absolute;
  }

  .switch-slider {
    position: absolute;
    inset: 0;
    background: #D9D5CC;
    border-radius: 999px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .switch-slider::before {
    content: '';
    position: absolute;
    width: 16px;
    height: 16px;
    left: 2px;
    top: 2px;
    background: #fff;
    border-radius: 50%;
    transition: transform 0.15s;
    box-shadow: 0 1px 2px rgba(0,0,0,0.2);
  }

  .switch input:checked + .switch-slider {
    background: var(--zc-text-primary, #1F1E1C);
  }

  .switch input:checked + .switch-slider::before {
    transform: translateX(14px);
  }

  /* ── Footer ── */
  .settings-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--zc-border-soft, #ECE9E2);
    flex-shrink: 0;
  }
</style>
