import { Store } from "@tauri-apps/plugin-store";

const STORE_FILE = "zcode-settings.json";

export interface AIProviderSettings {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export interface SkillsSettings {
  summarize: boolean;
  fixGrammar: boolean;
  generateToc: boolean;
  explainCode: boolean;
}

export interface AppSettings {
  aiProvider: AIProviderSettings;
  skills: SkillsSettings;
}

const DEFAULTS: AppSettings = {
  aiProvider: {
    baseUrl: "",
    apiKey: "",
    model: "",
  },
  skills: {
    summarize: true,
    fixGrammar: true,
    generateToc: false,
    explainCode: false,
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
      // Merge saved values over defaults, so new keys never break on upgrade
      return {
        aiProvider: { ...DEFAULTS.aiProvider, ...(saved.aiProvider ?? {}) },
        skills: { ...DEFAULTS.skills, ...(saved.skills ?? {}) },
      };
    }
  } catch {
    // Ignore load errors — return defaults
  }
  return { ...DEFAULTS, aiProvider: { ...DEFAULTS.aiProvider }, skills: { ...DEFAULTS.skills } };
}

/**
 * Save settings to disk.
 * NOTE: Values are stored as plain JSON. The API key will be written to
 * `zcode-settings.json` in cleartext — no OS-level encryption (keychain /
 * Credential Manager) is used. This is acceptable while the AI feature is
 * still stubbed out, but should be replaced with tauri-plugin-stronghold or a
 * system keyring if the key is ever used for real API calls.
 */
export async function save(settings: AppSettings): Promise<void> {
  try {
    const store = await getSettingsStore();
    await store.set("settings", settings);
    await store.save();
  } catch {
    // Ignore persist errors
  }
}
