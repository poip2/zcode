<script lang="ts">
  interface ToolConfirmation {
    callId: string;
    toolName: string;
    summary: string;
    details: Record<string, unknown>;
  }

  let {
    confirmation,
    onApprove,
    onReject,
    autoApproveWrites = false,
    onAutoApproveChange,
  }: {
    confirmation: ToolConfirmation;
    onApprove: (callId: string) => void;
    onReject: (callId: string) => void;
    autoApproveWrites?: boolean;
    onAutoApproveChange?: (value: boolean) => void;
  } = $props();

  let showFullDiff = $state(false);

  function formatDiff(oldText: string, newText: string): { lines: Array<{ type: "add" | "remove" | "context"; text: string }> } {
    // Simple line-by-line diff
    const oldLines = (oldText || "").split("\n");
    const newLines = (newText || "").split("\n");
    const result: Array<{ type: "add" | "remove" | "context"; text: string }> = [];

    // Show removed lines then added lines (simplified diff)
    for (const line of oldLines) {
      result.push({ type: "remove", text: line || " " });
    }
    for (const line of newLines) {
      result.push({ type: "add", text: line || " " });
    }
    return { lines: result };
  }

  let diffLines = $derived(
    confirmation.toolName === "edit" || confirmation.toolName === "write"
      ? formatDiff(
          confirmation.toolName === "edit"
            ? String(confirmation.details.oldText ?? "")
            : "",
          confirmation.toolName === "edit"
            ? String(confirmation.details.newText ?? "")
            : confirmation.toolName === "write"
              ? String(confirmation.details.content ?? "")
              : "",
        )
      : { lines: [] },
  );

  let commandPreview = $derived(
    confirmation.toolName === "shell"
      ? String(confirmation.details.command ?? "")
      : "",
  );

  let filePath = $derived(String(confirmation.details.path ?? ""));
</script>

<div class="confirm-overlay">
  <div class="confirm-dialog">
    <div class="confirm-header">
      <div class="confirm-icon">
        {#if confirmation.toolName === "write"}
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
            <polyline points="14 2 14 8 20 8"/>
            <line x1="12" y1="18" x2="12" y2="12"/>
            <line x1="9" y1="15" x2="15" y2="15"/>
          </svg>
        {:else if confirmation.toolName === "edit"}
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
            <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
            <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
          </svg>
        {:else}
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
            <polyline points="4 17 10 11 4 5"/>
            <line x1="12" y1="19" x2="20" y2="19"/>
          </svg>
        {/if}
      </div>
      <div class="confirm-title-section">
        <h3>Allow tool execution?</h3>
        <p class="confirm-subtitle">
          {confirmation.toolName === "write"
            ? "Agent wants to write a file"
            : confirmation.toolName === "edit"
              ? "Agent wants to edit a file"
              : "Agent wants to run a command"}
        </p>
      </div>
    </div>

    <div class="confirm-body">
      <!-- File path -->
      {#if filePath}
        <div class="detail-row">
          <span class="detail-label">File</span>
          <code class="detail-path">{filePath}</code>
        </div>
      {/if}

      <!-- Diff view for edit/write -->
      {#if diffLines.lines.length > 0}
        <div class="diff-section">
          <button
            class="diff-toggle"
            onclick={() => (showFullDiff = !showFullDiff)}
          >
            {showFullDiff ? "▲" : "▼"} {showFullDiff ? "Hide" : "Show"} changes
          </button>
          {#if showFullDiff}
            <div class="diff-view">
              {#each diffLines.lines as line}
                <div class="diff-line" class:diff-add={line.type === "add"} class:diff-remove={line.type === "remove"}>
                  <span class="diff-marker">{line.type === "add" ? "+" : line.type === "remove" ? "-" : " "}</span>
                  <span class="diff-text">{line.text}</span>
                </div>
              {/each}
            </div>
          {:else}
            <div class="diff-summary">{confirmation.summary}</div>
          {/if}
        </div>
      {/if}

      <!-- Command preview for shell -->
      {#if commandPreview}
        <div class="command-section">
          <span class="detail-label">Command</span>
          <pre class="command-preview">{commandPreview}</pre>
        </div>
      {/if}
    </div>

    <div class="confirm-footer">
      <label class="auto-approve-label">
        <input
          type="checkbox"
          checked={autoApproveWrites}
          onchange={(e) => onAutoApproveChange?.((e.target as HTMLInputElement).checked)}
        />
        <span>Auto-approve future write/edit/shell operations</span>
      </label>

      <div class="confirm-actions">
        <button class="btn-reject" onclick={() => onReject(confirmation.callId)}>
          Reject
        </button>
        <button class="btn-approve" onclick={() => onApprove(confirmation.callId)}>
          Approve
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .confirm-overlay {
    position: absolute;
    inset: 0;
    background: rgba(24, 21, 16, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 16px;
  }

  .confirm-dialog {
    background: var(--zc-bg-card, #FDFDFB);
    border-radius: 12px;
    box-shadow: 0 16px 44px rgba(0, 0, 0, 0.24);
    width: 100%;
    max-width: 480px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .confirm-header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 16px;
    border-bottom: 1px solid var(--zc-border-soft, #ECE9E2);
  }

  .confirm-icon {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #fef3c7;
    border-radius: 8px;
    color: #b45309;
  }

  .confirm-title-section h3 {
    font-size: 14px;
    font-weight: 600;
    color: var(--zc-text-primary, #1F1E1C);
    margin: 0;
  }

  .confirm-subtitle {
    font-size: 12px;
    color: var(--zc-text-secondary, #8A8782);
    margin: 2px 0 0;
  }

  .confirm-body {
    padding: 12px 16px;
    overflow-y: auto;
    flex: 1;
  }

  .detail-row {
    margin-bottom: 10px;
  }

  .detail-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--zc-text-tertiary, #A8A49D);
    display: block;
    margin-bottom: 4px;
  }

  .detail-path {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12px;
    color: var(--zc-text-primary, #1F1E1C);
    background: var(--zc-bg-chrome, #F4F2ED);
    padding: 3px 8px;
    border-radius: 4px;
    word-break: break-all;
  }

  .diff-section {
    margin-top: 8px;
  }

  .diff-toggle {
    font-size: 11px;
    font-weight: 500;
    background: none;
    border: none;
    color: var(--zc-text-secondary, #8A8782);
    cursor: pointer;
    padding: 4px 0;
  }

  .diff-toggle:hover {
    color: var(--zc-text-primary, #1F1E1C);
  }

  .diff-summary {
    font-size: 12px;
    color: var(--zc-text-secondary, #8A8782);
    padding: 8px;
    background: var(--zc-bg-chrome, #F4F2ED);
    border-radius: 6px;
    white-space: pre-wrap;
    font-family: "SF Mono", Menlo, monospace;
    line-height: 1.5;
    margin-top: 4px;
  }

  .diff-view {
    margin-top: 6px;
    border: 1px solid var(--zc-border-soft, #ECE9E2);
    border-radius: 6px;
    overflow: hidden;
    max-height: 240px;
    overflow-y: auto;
    font-family: "SF Mono", Menlo, monospace;
    font-size: 11.5px;
    line-height: 1.6;
  }

  .diff-line {
    display: flex;
    padding: 1px 8px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .diff-add {
    background: #ecfdf5;
    color: #065f46;
  }

  .diff-remove {
    background: #fef2f2;
    color: #991b1b;
  }

  .diff-marker {
    flex-shrink: 0;
    width: 16px;
    font-weight: 600;
    user-select: none;
  }

  .diff-text {
    flex: 1;
    min-width: 0;
  }

  .command-section {
    margin-top: 10px;
  }

  .command-preview {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12px;
    background: #1e1e1e;
    color: #d4d4d4;
    padding: 10px 12px;
    border-radius: 6px;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
    margin: 4px 0 0;
  }

  .confirm-footer {
    padding: 12px 16px;
    border-top: 1px solid var(--zc-border-soft, #ECE9E2);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .auto-approve-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--zc-text-secondary, #8A8782);
    cursor: pointer;
  }

  .auto-approve-label input[type="checkbox"] {
    accent-color: var(--zc-text-primary, #1F1E1C);
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn-reject {
    padding: 7px 16px;
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    border: 1px solid var(--zc-border, #E7E4DD);
    background: var(--zc-bg-card, #FDFDFB);
    color: var(--zc-text-primary, #1F1E1C);
    border-radius: 6px;
    cursor: pointer;
  }

  .btn-reject:hover {
    background: var(--zc-bg-chrome, #F4F2ED);
  }

  .btn-approve {
    padding: 7px 16px;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    border: none;
    background: var(--zc-text-primary, #1F1E1C);
    color: #fff;
    border-radius: 6px;
    cursor: pointer;
  }

  .btn-approve:hover {
    opacity: 0.88;
  }
</style>
