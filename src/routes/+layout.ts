import { load as loadSettings } from '$lib/stores/settings';

// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

export async function load() {
  try {
    const settings = await loadSettings();
    return { locale: settings.locale };
  } catch {
    return { locale: undefined };
  }
}
