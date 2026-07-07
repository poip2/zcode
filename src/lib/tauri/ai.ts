import { invoke } from "@tauri-apps/api/core";

// ============================================================================
// AI Provider Keychain Commands
// ============================================================================

/**
 * Store (or overwrite) the API key in the OS keychain.
 * Returns null on success, or a warning string if keychain unavailable.
 */
export async function saveApiKey(apiKey: string): Promise<string | null> {
  return invoke<string | null>("save_api_key", { apiKey });
}

/**
 * Reveal the real API key from the keychain.
 * Call ONLY when the user clicks the eye icon.
 */
export async function revealApiKey(): Promise<string> {
  return invoke<string>("reveal_api_key");
}

/**
 * Delete the API key from the keychain.
 */
export async function deleteApiKey(): Promise<string | null> {
  return invoke<string | null>("delete_api_key");
}

/**
 * Call the AI provider with a text prompt.
 * base_url + model from store, apiKey from keychain.
 */
export async function callAIProvider(
  baseUrl: string,
  model: string,
  prompt: string,
): Promise<string> {
  return invoke<string>("call_ai_provider", { baseUrl, model, prompt });
}

// ============================================================================
// Local utility
// ============================================================================

/** Mask a key for safe display/storage: "sk-abc123...xyz789" → "sk-***z789" */
export function maskApiKey(key: string): string {
  if (key.length <= 7) return "***";
  return key.slice(0, 3) + "***" + key.slice(-4);
}
