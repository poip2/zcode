<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getAgentSession, resolveSessionKey, type ChatMessage, type ToolConfirmation } from "$lib/stores/agentSession";
  import { load as loadSettings, save as saveSettings, type AIProviderSettings } from "$lib/stores/settings";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { document as docStore } from "$lib/stores/document";
  import { getBaseDir } from "$lib/tauri/files";
  import ToolConfirmDialog from "$lib/components/ToolConfirmDialog.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";

  type AgentSession = Awaited<ReturnType<typeof getAgentSession>>;

  let {
    filePath = null as string | null,
    onClose,
  }: {
    filePath?: string | null;
    onClose: () => void;
  } = $props();

  // Session key resolves reactively from filePath via $effect.
  // The component is destroyed/recreated by the parent's {#if} on panel open/close,
  // but filePath can also change while the panel stays open (switching tabs).
  let sessionId = $state("scratch");
  let session: AgentSession | undefined;

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

  let inputEl: HTMLTextAreaElement | undefined = $state();
  let scrollEl: HTMLDivElement | undefined = $state();
  let unsubMessages: (() => void) | undefined;
  let unsubPinned: () => void;
  let unsubDoc: () => void;
  let mounted = false;
  let loading = $state(true);

  // Reactive session switching: when filePath changes, resolve key and load session
  $effect(() => {
    const fp = filePath;
    (async () => {
      loading = true;
      const key = await resolveSessionKey(fp);
      sessionId = key;
      session = await getAgentSession(key);

      // Unsubscribe previous store if any
      unsubMessages?.();

      unsubMessages = session.state.subscribe((s: any) => {
        messages = s.messages;
        streamingText = s.streamingText;
        sending = s.sending;
        error = s.error;
        activeToolCall = s.activeToolCall;
        toolConfirmation = s.toolConfirmation;
      });

      loading = false;
    })();
  });

  onMount(async () => {
    mounted = true;

    const saved = await loadSettings();
    aiSettings = saved.aiProvider;
    autoApproveWrites = saved.aiProvider.autoApproveWrites ?? false;

    unsubPinned = pinnedFolder.subscribe((p) => {
      pinnedPath = p;
    });

    unsubDoc = docStore.subscribe((d) => {
      // We just need the current file path for context
    });

    // Focus input
    requestAnimationFrame(() => inputEl?.focus());
  });

  onDestroy(() => {
    mounted = false;
    unsubMessages?.();
    unsubPinned?.();
    unsubDoc?.();
  });

  function scrollToBottom() {
    requestAnimationFrame(() => {
      if (scrollEl) {
        scrollEl.scrollTop = scrollEl.scrollHeight;
      }
    });
  }

  // Auto-scroll when messages or streaming text change
  $effect(() => {
    // Access reactive deps to trigger effect
    void messages.length;
    void streamingText.length;
    scrollToBottom();
  });

  function handleApproveTool(callId: string) {
    session?.confirmTool(callId, true);
  }

  function handleRejectTool(callId: string) {
    session?.confirmTool(callId, false);
  }

  async function handleAutoApproveChange(value: boolean) {
    autoApproveWrites = value;
    // Persist to settings
    const s = await loadSettings();
    s.aiProvider.autoApproveWrites = value;
    await saveSettings(s);
  }

  async function handleSend() {
    const text = inputText.trim();
    if (!text || sending || !session) return;

    inputText = "";

    // Always reload settings from disk before sending to avoid using
    // stale values cached at mount time (e.g. user changed settings
    // while the panel was open).
    const freshSettings = await loadSettings();
    const doc = $docStore;

    // Derive cwd: current file's parent dir takes priority, fallback to pinned folder
    const derivedCwd = doc.filePath
      ? getBaseDir(doc.filePath)
      : (pinnedPath ?? undefined);

    await session.send(text, {
      baseUrl: freshSettings.aiProvider.baseUrl,
      model: freshSettings.aiProvider.model,
      cwd: derivedCwd,
      currentFile: doc.filePath ?? undefined,
      autoApproveWrites: freshSettings.aiProvider.autoApproveWrites ?? autoApproveWrites,
    });
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

  function handleReset() {
    session?.reset();
    inputText = "";
    requestAnimationFrame(() => inputEl?.focus());
  }

  function renderMessageContent(content: string): string {
    // Very basic markdown-like rendering for inline code and backticks
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

<div class="agent-panel">
  <!-- Header -->
  <div class="panel-header">
    <div class="header-left">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
        <circle cx="10" cy="12" r="1.4" fill="currentColor" stroke="none"/>
        <circle cx="14" cy="12" r="1.4" fill="currentColor" stroke="none"/>
        <circle cx="12" cy="7.5" r="1.4" fill="currentColor" stroke="none"/>
      </svg>
      <span class="header-title">AI Agent</span>
    </div>
    <div class="header-right">
      <button class="icon-btn" onclick={handleReset} title="Reset conversation">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 2v5h5"/>
          <path d="M14 14v-5H9"/>
          <path d="M13.5 6.5A6 6 0 0 0 2.8 4.2"/>
          <path d="M2.5 9.5A6 6 0 0 0 13.2 11.8"/>
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

  <!-- Messages -->
  <div class="messages-scroll" bind:this={scrollEl}>
    {#if messages.length === 0 && !sending}
      <div class="empty-state">
        <p>Ask me anything about your project.</p>
        <p class="empty-hint">I can read, write, and edit files, run commands, and search your codebase.</p>
      </div>
    {/if}

    {#each messages as msg (msg.id)}
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

  /* Header */
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
  }

  .header-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--zc-text-primary, #1F1E1C);
  }

  .header-right {
    display: flex;
    gap: 2px;
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

  /* Skills bar */
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

  /* Messages */
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

  /* Error banner */
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

  /* Input area */
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
