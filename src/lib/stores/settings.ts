import { Store } from "@tauri-apps/plugin-store";

const STORE_FILE = "zcode-settings.json";

/**
 * AI provider settings persisted to the store.
 *
 * `maskedApiKey` is a de-identified version (e.g. "sk-5d70d***5c60")
 * safe to store in plaintext. The real apiKey lives exclusively in
 * the OS keychain.
 */
export interface AIProviderSettings {
  baseUrl: string;
  model: string;
  /** De-identified key stored locally; real key in keychain */
  maskedApiKey?: string;
  /** Auto-approve write/edit/shell operations without confirmation */
  autoApproveWrites?: boolean;
}

export interface AppSettings {
  aiProvider: AIProviderSettings;
}

const DEFAULTS: AppSettings = {
  aiProvider: {
    baseUrl: "",
    model: "",
  },
};

let storePromise: Promise<Store> | null = null;

function getSettingsStore(): Promise<Store> {
  if (!storePromise) {
    storePromise = Store.load(STORE_FILE);
  }
  return storePromise;
}

/** Load settings from disk. Returns defaults for any missing keys. */
export async function load(): Promise<AppSettings> {
  try {
    const store = await getSettingsStore();
    const saved = await store.get<AppSettings>("settings");
    if (saved) {
      // Merge saved values over defaults, so new keys never break on upgrade.
      // maskedApiKey is safe to store — it's the de-identified version only.
      return {
        aiProvider: { ...DEFAULTS.aiProvider, ...(saved.aiProvider ?? {}) },
      };
    }
  } catch {
    // Ignore load errors — return defaults
  }
  return { aiProvider: { ...DEFAULTS.aiProvider } };
}

/**
 * Save settings to disk.
 *
 * The `maskedApiKey` field in aiProvider is a de-identified version
 * (e.g. "sk-5d70d***5c60") — safe to persist as plaintext. The real
 * apiKey is stored in the OS keychain via Rust commands.
 */
export async function save(settings: AppSettings): Promise<boolean> {
  try {
    const store = await getSettingsStore();
    await store.set("settings", settings);
    await store.save();
    return true;
  } catch {
    return false;
  }
}
