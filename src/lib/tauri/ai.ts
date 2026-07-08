import { invoke } from "@tauri-apps/api/core";

/**
 * Store (or overwrite) the API key in the OS keychain.
 * Pass empty string to delete.
 * Returns null on success, or a warning string if keychain unavailable.
 */
export async function saveApiKey(apiKey: string): Promise<string | null> {
  return invoke<string | null>("save_api_key", { apiKey });
}

/**
 * Call the AI provider with a text prompt.
 * base_url + model from store, apiKey from keychain.
 * providerName is optional label (defaults to "openai" on backend).
 */
export async function callAIProvider(
  baseUrl: string,
  model: string,
  prompt: string,
  providerName?: string,
): Promise<string> {
  return invoke<string>("call_ai_provider", {
    baseUrl,
    model,
    prompt,
    providerName: providerName ?? null,
  });
}

/** Mask a key for safe display/storage: "sk-abc123...xyz789" → "sk-***z789" */
export function maskApiKey(key: string): string {
  if (key.length <= 7) return "***";
  return key.slice(0, 3) + "***" + key.slice(-4);
}

/**
 * Start an agent turn with streaming events.
 * Events are emitted via Tauri listen() with prefix `agent://{sessionId}/`.
 */
export interface StartAgentTurnArgs {
  sessionId: string;
  userMessage: string;
  allowedTools: string[];
  activeSkills: string[];
  baseUrl: string;
  model: string;
  providerName?: string;
  currentFile?: string;
  cwd?: string;
  autoApproveWrites?: boolean;
}

export async function startAgentTurn(args: StartAgentTurnArgs): Promise<void> {
  return invoke<void>("start_agent_turn", {
    sessionId: args.sessionId,
    userMessage: args.userMessage,
    allowedTools: args.allowedTools,
    activeSkills: args.activeSkills,
    baseUrl: args.baseUrl,
    model: args.model,
    providerName: args.providerName ?? null,
    currentFile: args.currentFile ?? null,
    cwd: args.cwd ?? null,
    autoApproveWrites: args.autoApproveWrites ?? false,
  });
}

/**
 * Approve or reject a pending tool call (Phase 3).
 */
export async function approveToolCall(
  sessionId: string,
  callId: string,
  approved: boolean,
): Promise<void> {
  return invoke<void>("approve_tool_call", {
    sessionId,
    callId,
    approved,
  });
}
