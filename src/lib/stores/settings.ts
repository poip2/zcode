import { Store } from "@tauri-apps/plugin-store";
import { joinPath } from "$lib/tauri/files";

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

export interface WorkspaceFolders {
  pinFolder: string;
  scriptsFolder: string;
  sourcesFolder: string;
  outputFolder: string;
}

export interface AppSettings {
  aiProvider: AIProviderSettings;
  /** Folder for generated non-md files (word, pdf, etc.). Default: {dataDir}/output */
  outputFolder?: string;
  /** Default pin folder when none is explicitly selected. Default: {dataDir}/pin */
  pinFolder?: string;
  /** Folder for agent-written scripts (python, js, etc.). Default: {dataDir}/scripts */
  scriptsFolder?: string;
  /** Staging area for existing non-md files the user wants the agent to modify. Default: {dataDir}/sources */
  sourcesFolder?: string;
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
        outputFolder: saved.outputFolder,
        pinFolder: saved.pinFolder,
        scriptsFolder: saved.scriptsFolder,
        sourcesFolder: saved.sourcesFolder,
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
    _notifyListeners(settings);
    return true;
  } catch {
    return false;
  }
}

type ChangeListener = () => void;
let changeListeners: ChangeListener[] = [];

export function onSettingsChange(cb: ChangeListener): () => void {
  changeListeners.push(cb);
  return () => {
    changeListeners = changeListeners.filter((l) => l !== cb);
  };
}

function _notifyListeners(_settings: AppSettings) {
  for (const cb of changeListeners) cb();
}

export async function resolveWorkspaceFolders(settings: AppSettings, dataDir: string): Promise<WorkspaceFolders> {
  return {
    pinFolder: settings.pinFolder || await joinPath(dataDir, "pin"),
    scriptsFolder: settings.scriptsFolder || await joinPath(dataDir, "scripts"),
    sourcesFolder: settings.sourcesFolder || await joinPath(dataDir, "sources"),
    outputFolder: settings.outputFolder || await joinPath(dataDir, "output"),
  };
}
