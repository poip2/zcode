import { writable, get } from "svelte/store";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { startAgentTurn, approveToolCall, type StartAgentTurnArgs } from "$lib/tauri/ai";

// ============================================================================
// Types
// ============================================================================

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "tool" | "error";
  content: string;
  toolName?: string;
  isToolError?: boolean;
  inputTokens?: number;
  outputTokens?: number;
  timestamp: number;
}

export interface ToolConfirmation {
  callId: string;
  toolName: string;
  summary: string;
  details: Record<string, unknown>;
}

interface SessionState {
  messages: ChatMessage[];
  streamingText: string;
  sending: boolean;
  activeToolCall: { callId: string; toolName: string } | null;
  error: string | null;
  turnCount: number;
  /** Pending tool confirmation (Phase 3) */
  toolConfirmation: ToolConfirmation | null;
}

// ============================================================================
// Session management
// ============================================================================

const sessions = new Map<string, ReturnType<typeof createSession>>();

function createSession(sessionId: string, initialMessages: ChatMessage[] = []) {
  const state = writable<SessionState>({
    messages: initialMessages,
    streamingText: "",
    sending: false,
    activeToolCall: null,
    error: null,
    turnCount: 0,
    toolConfirmation: null,
  });

  let unlisteners: UnlistenFn[] = [];
  let stopped = false;

  function cleanup() {
    for (const unlisten of unlisteners) {
      unlisten();
    }
    unlisteners = [];
  }

  async function setupListeners() {
    cleanup();
    const prefix = `agent://${sessionId}`;

    const u1 = await listen<{ delta: string }>(`${prefix}/token`, (event) => {
      state.update((s) => ({
        ...s,
        streamingText: s.streamingText + event.payload.delta,
      }));
    });

    const u2 = await listen<{
      callId: string;
      toolName: string;
      arguments: Record<string, unknown>;
      skillName?: string;
    }>(`${prefix}/tool-call`, (event) => {
      console.log("[tool-call payload]", JSON.stringify(event.payload));
      const { callId, toolName, skillName } = event.payload;

      const dangerousTools = ["write", "edit", "shell"];
      let initialContent: string;
      if (skillName) {
        initialContent = `📋 Reading skill: \`${skillName}\`…`;
      } else if (dangerousTools.includes(toolName)) {
        initialContent = `⏳ Awaiting approval for \`${toolName}\`…`;
      } else {
        initialContent = `📖 Reading with \`${toolName}\`…`;
      }

      const toolMsg: ChatMessage = {
        id: `tool-${callId}`,
        role: "tool",
        content: initialContent,
        toolName: skillName ?? toolName,
        isToolError: false,
        timestamp: Date.now(),
      };
      state.update((s) => ({
        ...s,
        messages: [...s.messages, toolMsg],
        activeToolCall: { callId, toolName },
      }));
    });

    const u3 = await listen<{
      callId: string;
      toolName: string;
      isError: boolean;
      summary: string;
      skillName?: string;
    }>(`${prefix}/tool-result`, (event) => {
      console.log("[tool-result payload]", JSON.stringify(event.payload));
      const { callId, toolName, isError, summary, skillName } = event.payload;
      state.update((s) => {
        const msgs = s.messages.map((m) => {
          if (m.id === `tool-${callId}`) {
            let newContent: string;
            if (skillName) {
              newContent = `📋 \`${skillName}\`: ${summary}`;
            } else if (isError) {
              newContent = `❌ \`${toolName}\` failed: ${summary}`;
            } else if (m.content.startsWith("📖")) {
              newContent = `📖 \`${toolName}\`: ${summary}`;
            } else {
              newContent = `✅ \`${toolName}\`: ${summary}`;
            }
            return {
              ...m,
              content: newContent,
              isToolError: isError,
            };
          }
          return m;
        });
        return { ...s, messages: msgs, activeToolCall: null };
      });
    });

    const u4 = await listen<{
      stopReason: string;
      inputTokens: number;
      outputTokens: number;
    }>(`${prefix}/turn-end`, (event) => {
      const { stopReason, inputTokens, outputTokens } = event.payload;
      state.update((s) => {
        const msgs = [...s.messages];
        if (s.streamingText.trim()) {
          msgs.push({
            id: `assistant-${Date.now()}`,
            role: "assistant",
            content: s.streamingText,
            inputTokens,
            outputTokens,
            timestamp: Date.now(),
          });
        } else if (stopReason === "Error") {
          msgs.push({
            id: `error-${Date.now()}`,
            role: "error",
            content: s.error || "An unknown error occurred",
            timestamp: Date.now(),
          });
        }
        return {
          ...s,
          messages: msgs,
          streamingText: "",
          sending: false,
          error: null,
          turnCount: s.turnCount + 1,
          toolConfirmation: null,
        };
      });
    });

    const u5 = await listen<{ message: string }>(`${prefix}/error`, (event) => {
      state.update((s) => ({
        ...s,
        error: event.payload.message,
      }));
    });

    // Phase 3: tool confirmation events
    const u6 = await listen<ToolConfirmation>(`${prefix}/tool-confirmation`, (event) => {
      console.log("[tool-confirmation payload]", JSON.stringify(event.payload));
      state.update((s) => ({
        ...s,
        toolConfirmation: event.payload,
      }));
    });

    unlisteners = [u1, u2, u3, u4, u5, u6];
  }

  async function send(
    userMessage: string,
    settings: {
      baseUrl: string;
      model: string;
      providerName?: string;
      currentFile?: string;
      cwd?: string;
      autoApproveWrites?: boolean;
    },
  ) {
    const s = get(state);
    if (s.sending) return;

    const userMsg: ChatMessage = {
      id: `user-${Date.now()}`,
      role: "user",
      content: userMessage,
      timestamp: Date.now(),
    };

    state.update((prev) => ({
      ...prev,
      messages: [...prev.messages, userMsg],
      streamingText: "",
      sending: true,
      error: null,
      activeToolCall: null,
      toolConfirmation: null,
    }));

    stopped = false;

    if (unlisteners.length === 0) {
      await setupListeners();
    }

    const args: StartAgentTurnArgs = {
      sessionId,
      userMessage,
      allowedTools: ["read", "write", "edit", "shell", "grep", "find", "ls"],
      baseUrl: settings.baseUrl,
      model: settings.model,
      providerName: settings.providerName,
      currentFile: settings.currentFile,
      cwd: settings.cwd,
      autoApproveWrites: settings.autoApproveWrites,
    };

    try {
      await startAgentTurn(args);
    } catch (err) {
      state.update((s) => ({
        ...s,
        sending: false,
        error: String(err),
        messages: [
          ...s.messages,
          {
            id: `error-${Date.now()}`,
            role: "error",
            content: `Failed to start agent: ${err}`,
            timestamp: Date.now(),
          },
        ],
      }));
    }
  }

  async function confirmTool(callId: string, approved: boolean) {
    try {
      await approveToolCall(sessionId, callId, approved);
      state.update((s) => ({ ...s, toolConfirmation: null }));
    } catch (err) {
      console.error("Failed to send tool approval:", err);
    }
  }

  function stop() {
    stopped = true;
    state.update((s) => ({ ...s, sending: false }));
  }

  async function reset() {
    stopped = false;
    state.set({
      messages: [],
      streamingText: "",
      sending: false,
      activeToolCall: null,
      error: null,
      turnCount: 0,
      toolConfirmation: null,
    });
    cleanup();
    // Also clear the persisted session on disk
    invoke("clear_session", { sessionKey: sessionId }).catch(() => {});
  }

  return {
    state,
    send,
    stop,
    reset,
    confirmTool,
    cleanup,
  };
}

// ============================================================================
// Public API
// ============================================================================

export async function getAgentSession(sessionId: string) {
  if (sessions.has(sessionId)) return sessions.get(sessionId)!;

  // Load persisted history from disk
  let history: ChatMessage[] = [];
  try {
    history = await invoke<ChatMessage[]>("load_session_messages", { sessionKey: sessionId });
  } catch (err) {
    console.error("Failed to load session history:", err);
  }

  const session = createSession(sessionId, history);
  sessions.set(sessionId, session);
  return session;
}

export async function resolveSessionKey(filePath: string | null): Promise<string> {
  if (!filePath) return "scratch";
  try {
    return await invoke<string>("resolve_session_key", { filePath });
  } catch {
    return "scratch";
  }
}

export function closeAgentSession(sessionId: string) {
  const session = sessions.get(sessionId);
  if (session) {
    session.cleanup();
    sessions.delete(sessionId);
  }
}
