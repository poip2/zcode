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
