<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getAgentSession, resolveSessionKey, loadSessionMessages, listSessions, closeAllSessions, type ChatMessage, type ToolConfirmation, type SessionMeta } from "$lib/stores/agentSession";
  import { invoke } from "@tauri-apps/api/core";
  import { load as loadSettings, save as saveSettings, resolveWorkspaceFolders, type AIProviderSettings } from "$lib/stores/settings";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { document as docStore } from "$lib/stores/document";
  import { getBaseDir, getDefaultDataDir } from "$lib/tauri/files";
  import ToolConfirmDialog from "$lib/components/ToolConfirmDialog.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import { reloadOutputFiles, reloadSourcesFiles } from "$lib/stores/workspaceFiles";
  import { folderTree } from "$lib/stores/folderTree";

  type AgentSession = Awaited<ReturnType<typeof getAgentSession>>;

  let {
    filePath = null as string | null,
    onClose,
  }: {
    filePath?: string | null;
    onClose: () => void;
  } = $props();

  // Active session (derived from filePath, overridden by history selection)
  let sessionId = $state("scratch");
  let session: AgentSession | undefined;

  // History state
  let showHistory = $state(false);
  let sessionList = $state<SessionMeta[]>([]);
  let searchQuery = $state("");

  // Message state
  let messages = $state<ChatMessage[]>([]);
  let streamingText = $state("");
  let sending = $state(false);
  let error = $state<string | null>(null);
  let activeToolCall = $state<{ callId: string; toolName: string } | null>(null);
  let toolConfirmation = $state<ToolConfirmation | null>(null);
  let autoApproveWrites = $state(false);

  let inputText = $state("");
  let aiSettings = $state<AIProviderSettings>({ baseUrl: "", model: "" });
  let pinnedPath = $state<string | null>(null);
  let lastOutputFolder = $state<string | null>(null);
  let lastSourcesFolder = $state<string | null>(null);
  let wasSending = $state(false);
  let lastToolCompletion = $state<{ toolName: string; isError: boolean; ts: number } | null>(null);

  // Stable workspace root for session grouping.
  // Falls back: open file's parent dir → pinned folder → null (scratch).
  function parentDir(p: string): string {
    const sep = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
    return sep > 0 ? p.slice(0, sep) : '.';
  }
  let workspaceCwd = $derived<string | null>(
    filePath ? parentDir(filePath) : (pinnedPath ?? null)
  );

  let inputEl: HTMLTextAreaElement | undefined = $state();
  let scrollEl: HTMLDivElement | undefined = $state();
  let unsubMessages: (() => void) | undefined;
  let unsubPinned: () => void;
  let unsubDoc: () => void;
  let mounted = false;
  let loading = $state(true);

  // Derived: panel title from first user message
  let panelTitle = $derived(
    messages.length === 0
      ? "AI Agent"
      : (messages.find(m => m.role === "user")?.content.trim().slice(0, 40) ?? "AI Agent") +
        (messages.length > 1 ? "…" : "")
  );

  // Derived: filtered session list
  let filteredSessions = $derived(
    searchQuery.trim()
      ? sessionList.filter(s =>
          s.title.toLowerCase().includes(searchQuery.toLowerCase())
        )
      : sessionList
  );

  // ------------------------------------------------------------------
  // Session switching
  // ------------------------------------------------------------------

  $effect(() => {
    const cwd = workspaceCwd;
    let cancelled = false;

    (async () => {
      loading = true;
      const key = await resolveSessionKey(cwd);
      if (cancelled) return;

      sessionId = key;
      session = await getAgentSession(key);
      if (cancelled) return;

      unsubMessages?.();
      unsubMessages = session.state.subscribe((s: any) => {
        messages = s.messages;
        streamingText = s.streamingText;
        sending = s.sending;
        error = s.error;
        activeToolCall = s.activeToolCall;
        toolConfirmation = s.toolConfirmation;
        lastToolCompletion = s.lastToolCompletion;
      });

      loading = false;
    })();

    return () => {
      cancelled = true;
      unsubMessages?.();
    };
  });

  async function loadSessionList() {
    sessionList = await listSessions(workspaceCwd ?? undefined);
  }

  async function switchToHistorySession(key: string) {
    // Don't switch if the session is already active
    if (key === sessionId) {
      showHistory = false;
      return;
    }

    // Load messages from disk
    const msgs = await loadSessionMessages(key);
    if (msgs.length === 0) {
      showHistory = false;
      return;
    }

    // Close current session listeners, open the new one (pass pre-loaded msgs to avoid re-reading)
    session = await getAgentSession(key, msgs);
    unsubMessages?.();
    unsubMessages = session.state.subscribe((s: any) => {
      messages = s.messages;
      streamingText = s.streamingText;
      sending = s.sending;
      error = s.error;
      activeToolCall = s.activeToolCall;
      toolConfirmation = s.toolConfirmation;
      lastToolCompletion = s.lastToolCompletion;
    });

    sessionId = key;
    showHistory = false;
    scrollToBottom();
  }

  function toggleHistory() {
    showHistory = !showHistory;
    if (showHistory) {
      loadSessionList();
      searchQuery = "";
    }
  }

  async function handleNewAgent() {
    showHistory = false;
    // Create a brand-new session key for the current folder.
    // The old session continues running in the background — we don't kill it.
    const effectiveCwd = workspaceCwd ?? "scratch";
    let newKey: string;
    try {
      newKey = await invoke<string>("new_session_key", { cwd: effectiveCwd });
    } catch {
      // Fallback: resolve normally (Rust will create a new key if none exists)
      newKey = await resolveSessionKey(effectiveCwd);
    }
    session = await getAgentSession(newKey);
    unsubMessages?.();
    unsubMessages = session.state.subscribe((s: any) => {
      messages = s.messages;
      streamingText = s.streamingText;
      sending = s.sending;
      error = s.error;
      activeToolCall = s.activeToolCall;
      toolConfirmation = s.toolConfirmation;
      lastToolCompletion = s.lastToolCompletion;
    });
    sessionId = newKey;
    inputText = "";
    requestAnimationFrame(() => inputEl?.focus());
  }

  // ------------------------------------------------------------------
  // Lifecycle
  // ------------------------------------------------------------------

  onMount(async () => {
    mounted = true;

    const saved = await loadSettings();
    aiSettings = saved.aiProvider;
    autoApproveWrites = saved.aiProvider.autoApproveWrites ?? false;

    unsubPinned = pinnedFolder.subscribe((p) => {
      pinnedPath = p;
    });

    unsubDoc = docStore.subscribe((_d) => {
      // We just need the current file path for context
    });

    // Preload session list for history icon
    loadSessionList();

    // Focus input
    requestAnimationFrame(() => inputEl?.focus());
  });

  onDestroy(() => {
    mounted = false;
    unsubMessages?.();
    unsubPinned?.();
    unsubDoc?.();
    closeAllSessions();
  });

  // ------------------------------------------------------------------
  // Helpers
  // ------------------------------------------------------------------

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (scrollEl) {
        scrollEl.scrollTop = scrollEl.scrollHeight;
      }
    });
  }

  $effect(() => {
    void messages.length;
    void streamingText.length;
    scrollToBottom();
  });

  $effect(() => {
    if (wasSending && !sending && lastOutputFolder) {
      reloadOutputFiles(lastOutputFolder).catch(() => {});
    }
    wasSending = sending;
  });

  // Debounced file list refresh on every tool completion
  let refreshDebounce: ReturnType<typeof setTimeout> | null = null;
  function scheduleFileListRefresh() {
    if (refreshDebounce) clearTimeout(refreshDebounce);
    refreshDebounce = setTimeout(() => {
      folderTree.refresh().catch(() => {});
      if (lastSourcesFolder) reloadSourcesFiles(lastSourcesFolder).catch(() => {});
      if (lastOutputFolder) reloadOutputFiles(lastOutputFolder).catch(() => {});
    }, 300);
  }

  $effect(() => {
    const completion = lastToolCompletion;
    if (completion && ["write", "edit", "shell"].includes(completion.toolName) && !completion.isError) {
      scheduleFileListRefresh();
    }
  });

  function relativeTime(ts: number): string {
    const diff = Date.now() - ts;
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 7) return `${days}d ago`;
    return new Date(ts).toLocaleDateString();
  }

  // ------------------------------------------------------------------
  // Tool approval
  // ------------------------------------------------------------------

  function handleApproveTool(callId: string) {
    session?.confirmTool(callId, true);
  }

  function handleRejectTool(callId: string) {
    session?.confirmTool(callId, false);
  }

  async function handleAutoApproveChange(value: boolean) {
    autoApproveWrites = value;
    const s = await loadSettings();
    s.aiProvider.autoApproveWrites = value;
    await saveSettings(s);
  }

  // ------------------------------------------------------------------
  // Send / Stop / Reset
  // ------------------------------------------------------------------

  async function handleSend() {
    const text = inputText.trim();
    if (!text || sending || !session) return;

    inputText = "";

    const freshSettings = await loadSettings();
    const doc = $docStore;

    const derivedCwd = doc.filePath
      ? getBaseDir(doc.filePath)
      : (pinnedPath ?? undefined);

    // Resolve folder paths: use settings values, or compute defaults
    try {
      const defaultDataDir = await getDefaultDataDir();
      const { pinFolder, scriptsFolder, sourcesFolder, outputFolder } = await resolveWorkspaceFolders(freshSettings, defaultDataDir);
      lastOutputFolder = outputFolder;
      lastSourcesFolder = sourcesFolder;

      await session.send(text, {
        baseUrl: freshSettings.aiProvider.baseUrl,
        model: freshSettings.aiProvider.model,
        cwd: derivedCwd,
        currentFile: doc.filePath ?? undefined,
        autoApproveWrites: freshSettings.aiProvider.autoApproveWrites ?? autoApproveWrites,
        pinFolder,
        scriptsFolder,
        sourcesFolder,
        outputFolder,
      });
    } catch {
      await session.send(text, {
        baseUrl: freshSettings.aiProvider.baseUrl,
        model: freshSettings.aiProvider.model,
        cwd: derivedCwd,
        currentFile: doc.filePath ?? undefined,
        autoApproveWrites: freshSettings.aiProvider.autoApproveWrites ?? autoApproveWrites,
      });
    }
  }

  function handleStop() {
    session?.stop();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function renderMessageContent(content: string): string {
    return content
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\*([^*]+)\*/g, "<em>$1</em>")
      .replace(/\n/g, "<br>");
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && showHistory && (showHistory = false)} />
<div class="agent-panel">
  <!-- ============================================================== -->
  <!-- Header: dropdown title + actions                               -->
  <!-- ============================================================== -->
  <div class="panel-header">
    <div class="header-left">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
        <circle cx="10" cy="12" r="1.4" fill="currentColor" stroke="none"/>
        <circle cx="14" cy="12" r="1.4" fill="currentColor" stroke="none"/>
        <circle cx="12" cy="7.5" r="1.4" fill="currentColor" stroke="none"/>
      </svg>
      <button class="title-dropdown-btn" onclick={toggleHistory} title="Conversation history">
        <span class="header-title">{panelTitle}</span>
        <svg class="title-caret" width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="4,6 8,10 12,6"/>
        </svg>
      </button>
    </div>
    <div class="header-right">
      <button class="icon-btn" onclick={handleNewAgent} title="New Agent">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <line x1="8" y1="3" x2="8" y2="13"/>
          <line x1="3" y1="8" x2="13" y2="8"/>
        </svg>
      </button>
      <button class="icon-btn" onclick={toggleHistory} title="History">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="8" cy="8.5" r="6"/>
          <path d="M8 5.2v3.3l2.4 1.4"/>
        </svg>
      </button>
      <button class="icon-btn" onclick={onClose} title="Close panel">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <line x1="4" y1="4" x2="12" y2="12"/>
          <line x1="12" y1="4" x2="4" y2="12"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- Active skills bar -->
  {#if skillsStore.enabled.length > 0}
    <div class="skills-bar">
      {#each skillsStore.enabled as skill}
        <span class="skill-tag">{skill.name}</span>
      {/each}
    </div>
  {/if}

  <!-- ============================================================== -->
  <!-- Messages area (with history flyout overlay)                    -->
  <!-- ============================================================== -->
  <div class="messages-wrapper">
    <div class="messages-scroll" bind:this={scrollEl}>
      {#if messages.length === 0 && !sending}
        <div class="empty-state">
          <p>Ask me anything about your project.</p>
          <p class="empty-hint">I can read, write, and edit files, run commands, and search your codebase.</p>
        </div>
      {/if}

      {#each messages as msg (msg.id)}
        {#if msg.role === "user" || msg.role === "assistant" || msg.role === "tool" || msg.role === "error"}
          <div class="message" class:user={msg.role === "user"} class:tool={msg.role === "tool"} class:error={msg.role === "error"}>
            <div class="msg-role">
              {#if msg.role === "user"}
                <span class="role-icon">👤</span>
              {:else if msg.role === "assistant"}
                <span class="role-icon">🤖</span>
              {:else if msg.role === "tool"}
                <span class="role-icon">🔧</span>
              {:else if msg.role === "error"}
                <span class="role-icon">⚠️</span>
              {/if}
            </div>
            <div class="msg-body">
              {@html renderMessageContent(msg.content)}
              {#if msg.role === "assistant" && msg.inputTokens !== undefined}
                <div class="msg-usage">
                  {msg.inputTokens}↑ {msg.outputTokens}↓ tokens
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {/each}

      <!-- Streaming text -->
      {#if streamingText || (sending && streamingText === "")}
        <div class="message assistant">
          <div class="msg-role">
            <span class="role-icon">🤖</span>
          </div>
          <div class="msg-body">
            {#if streamingText}
              {@html renderMessageContent(streamingText)}<span class="cursor-blink">▌</span>
            {:else}
              <span class="thinking">Thinking…</span>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Active tool indicator -->
      {#if activeToolCall}
        <div class="message tool">
          <div class="msg-role">
            <span class="role-icon spinning">⚙️</span>
          </div>
          <div class="msg-body">
            <span class="tool-working">Running <code>{activeToolCall.toolName}</code>…</span>
          </div>
        </div>
      {/if}
    </div>

    <!-- ============================================================ -->
    <!-- History flyout (overlay on messages)                         -->
    <!-- ============================================================ -->
    {#if showHistory}
      <!-- Backdrop to catch clicks outside -->
      <div
        class="history-backdrop"
        onclick={toggleHistory}
        role="presentation"
      ></div>
      <div class="history-flyout">
        <div class="history-search">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="search-icon">
            <circle cx="7" cy="7" r="4.5"/>
            <line x1="11" y1="11" x2="15" y2="15"/>
          </svg>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type="text"
            placeholder="Search conversations…"
            bind:value={searchQuery}
            autofocus
          />
        </div>
        <button class="history-new-btn" onclick={handleNewAgent}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <line x1="8" y1="3" x2="8" y2="13"/>
            <line x1="3" y1="8" x2="13" y2="8"/>
          </svg>
          New Agent
        </button>
        <div class="history-list">
          {#each filteredSessions as s (s.sessionKey)}
            <button
              class="history-item"
              class:active={s.sessionKey === sessionId}
              onclick={() => switchToHistorySession(s.sessionKey)}
            >
              <span class="history-item-title">{s.title}</span>
              <span class="history-item-meta">{relativeTime(s.timestamp)} · {s.messageCount} messages</span>
            </button>
          {:else}
            <div class="history-empty">
              {searchQuery ? "No matching conversations" : "No conversations yet"}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <!-- Error banner -->
  {#if error}
    <div class="error-banner">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      <span>{error}</span>
    </div>
  {/if}

  <!-- Tool confirmation dialog (Phase 3) -->
  {#if toolConfirmation}
    <ToolConfirmDialog
      confirmation={toolConfirmation}
      autoApproveWrites={autoApproveWrites}
      onApprove={handleApproveTool}
      onReject={handleRejectTool}
      onAutoApproveChange={handleAutoApproveChange}
    />
  {/if}

  <!-- Input area -->
  <div class="input-area">
    <textarea
      bind:this={inputEl}
      bind:value={inputText}
      class="chat-input"
      placeholder={sending ? "Agent is responding…" : "Ask something… (Enter to send, Shift+Enter for new line)"}
      rows="2"
      onkeydown={handleKeydown}
      disabled={sending}
    ></textarea>
    <div class="input-actions">
      {#if sending}
        <button class="stop-btn" onclick={handleStop} title="Stop generation">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <rect x="3" y="3" width="10" height="10" rx="1"/>
          </svg>
          Stop
        </button>
      {:else}
        <button
          class="send-btn"
          onclick={handleSend}
          disabled={!inputText.trim()}
          title="Send message"
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <line x1="8" y1="2" x2="8" y2="14"/>
            <polyline points="4,8 8,2 12,8"/>
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .agent-panel {
    position: fixed;
    bottom: 80px;
    right: 20px;
    z-index: 950;
    width: 360px;
    height: 480px;
    max-height: calc(100vh - 100px);
    display: flex;
    flex-direction: column;
    background: var(--zc-bg-card, #FDFDFB);
    border: 1px solid var(--zc-border, #E7E4DD);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0,0,0,0.14), 0 2px 8px rgba(0,0,0,0.08);
    overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }

  /* ── Header ── */
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
    flex-shrink: 0;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .header-left > svg {
    flex-shrink: 0;
    color: var(--zc-text-secondary, #8A8782);
  }

  .title-dropdown-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    border: none;
    background: none;
    cursor: pointer;
    font-family: inherit;
    padding: 2px 4px;
    border-radius: 4px;
    min-width: 0;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .title-dropdown-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
  }

  .header-title {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 160px;
  }

  .title-caret {
    color: var(--zc-text-tertiary, #A8A49D);
    flex-shrink: 0;
  }

  .header-right {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    color: var(--zc-text-tertiary, #A8A49D);
    cursor: pointer;
    border-radius: 4px;
  }

  .icon-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
    color: var(--zc-text-primary, #1F1E1C);
  }

  /* ── Skills bar ── */
  .skills-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
    flex-shrink: 0;
  }

  .skill-tag {
    font-size: 10px;
    font-weight: 500;
    padding: 2px 6px;
    background: #f0f4ff;
    color: #4a6fa5;
    border: 1px solid #d0ddf5;
    border-radius: 4px;
  }

  /* ── Messages wrapper (positioned for flyout) ── */
  .messages-wrapper {
    flex: 1;
    position: relative;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* ── Messages ── */
  .messages-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-height: 200px;
    text-align: center;
    color: var(--zc-text-tertiary, #A8A49D);
    gap: 6px;
  }

  .empty-state p {
    font-size: 13px;
    margin: 0;
  }

  .empty-hint {
    font-size: 11px !important;
    max-width: 240px;
    line-height: 1.4;
  }

  .message {
    display: flex;
    gap: 8px;
    max-width: 100%;
  }

  .message.user {
    flex-direction: row-reverse;
  }

  .msg-role {
    flex-shrink: 0;
    width: 24px;
    text-align: center;
  }

  .role-icon {
    font-size: 14px;
  }

  .msg-body {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    line-height: 1.55;
    color: var(--zc-text-primary, #1F1E1C);
    padding: 8px 10px;
    border-radius: 8px;
    word-break: break-word;
  }

  .message.user .msg-body {
    background: var(--zc-text-primary, #1F1E1C);
    color: #fff;
  }

  .message.assistant .msg-body {
    background: var(--zc-bg-chrome, #F4F2ED);
  }

  .message.tool .msg-body {
    background: transparent;
    font-size: 11.5px;
    color: var(--zc-text-tertiary, #A8A49D);
    padding: 4px 0;
  }

  .message.error .msg-body {
    background: #fff5f5;
    border: 1px solid #fecaca;
    color: #b91c1c;
    font-size: 12px;
  }

  .msg-body :global(code) {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12px;
    background: rgba(0,0,0,0.06);
    padding: 1px 5px;
    border-radius: 3px;
  }

  .message.user .msg-body :global(code) {
    background: rgba(255,255,255,0.15);
  }

  .msg-usage {
    margin-top: 4px;
    font-size: 10px;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .cursor-blink {
    animation: blink 1s step-end infinite;
    color: var(--zc-text-primary, #1F1E1C);
  }

  @keyframes blink {
    50% { opacity: 0; }
  }

  .thinking {
    color: var(--zc-text-tertiary, #A8A49D);
    font-style: italic;
  }

  .spinning {
    display: inline-block;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .tool-working {
    font-size: 11.5px;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  /* ── History flyout ── */
  .history-backdrop {
    position: absolute;
    inset: 0;
    z-index: 10;
    background: rgba(24, 21, 16, 0.15);
  }

  .history-flyout {
    position: absolute;
    inset: 0;
    z-index: 11;
    display: flex;
    flex-direction: column;
    background: var(--zc-bg-card, #FDFDFB);
    overflow: hidden;
  }

  .history-search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
    flex-shrink: 0;
  }

  .search-icon {
    flex-shrink: 0;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .history-search input {
    flex: 1;
    padding: 5px 4px;
    border: none;
    font-size: 12px;
    font-family: inherit;
    outline: none;
    background: transparent;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .history-search input::placeholder {
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .history-new-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: calc(100% - 16px);
    margin: 4px 8px;
    padding: 7px 8px;
    border: none;
    border-radius: 6px;
    background: none;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--zc-text-primary, #1F1E1C);
    cursor: pointer;
    font-family: inherit;
    flex-shrink: 0;
  }

  .history-new-btn:hover {
    background: var(--zc-active-row, #EAE6DD);
  }

  .history-list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 0 4px;
  }

  .history-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 7px 12px;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
    border-radius: 4px;
  }

  .history-item:hover {
    background: var(--zc-active-row, #EAE6DD);
  }

  .history-item.active {
    background: var(--zc-active-row, #EAE6DD);
  }

  .history-item-title {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--zc-text-primary, #1F1E1C);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .history-item-meta {
    font-size: 10.5px;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .history-empty {
    padding: 24px 16px;
    text-align: center;
    font-size: 12px;
    color: var(--zc-text-tertiary, #A8A49D);
  }

  /* ── Error banner ── */
  .error-banner {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px 12px;
    font-size: 11px;
    color: #b45309;
    background: #fffbeb;
    border-top: 1px solid #fde68a;
    border-bottom: 1px solid #fde68a;
    flex-shrink: 0;
    line-height: 1.4;
  }

  /* ── Input area ── */
  .input-area {
    padding: 10px 12px 12px;
    border-top: 1px solid var(--zc-border-soft, #ECE9E2);
    flex-shrink: 0;
  }

  .chat-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--zc-border, #E7E4DD);
    border-radius: 8px;
    font-size: 13px;
    font-family: inherit;
    line-height: 1.5;
    color: var(--zc-text-primary, #1F1E1C);
    background: var(--zc-bg-card, #FDFDFB);
    resize: none;
    outline: none;
    box-sizing: border-box;
  }

  .chat-input:focus {
    border-color: #B9B6B0;
  }

  .chat-input:disabled {
    background: var(--zc-bg-chrome, #F4F2ED);
    color: var(--zc-text-tertiary, #A8A49D);
  }

  .input-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 6px;
    gap: 6px;
  }

  .send-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 8px;
    background: var(--zc-text-primary, #1F1E1C);
    color: #fff;
    cursor: pointer;
  }

  .send-btn:hover {
    opacity: 0.88;
  }

  .send-btn:disabled {
    background: var(--zc-border, #E7E4DD);
    cursor: not-allowed;
  }

  .stop-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    border: 1px solid #e03e3e;
    background: #fff5f5;
    color: #e03e3e;
    border-radius: 6px;
    cursor: pointer;
  }

  .stop-btn:hover {
    background: #fee2e2;
  }
</style>
