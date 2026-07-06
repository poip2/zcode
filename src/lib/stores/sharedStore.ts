import { Store } from "@tauri-apps/plugin-store";

let storePromise: Promise<Store> | null = null;

export function getStore(): Promise<Store> {
  if (!storePromise) {
    storePromise = Store.load("zcode-recents.json");
  }
  return storePromise;
}
