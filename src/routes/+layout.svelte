<script lang="ts">
  import { onMount } from "svelte";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { document as docStore } from "$lib/stores/document";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import { startSkillsWatcher, stopSkillsWatcher, listenSkillsChanged } from "$lib/tauri/watcher";
  import { getBaseDir, getDefaultDataDir, joinPath, pathExists, createFolder } from "$lib/tauri/files";
  import { load as loadSettings, save as saveSettings, resolveWorkspaceFolders } from "$lib/stores/settings";
  import { locale } from "$lib/i18n";
  import "../app.css";

  let { children } = $props();

  let currentCwd = $state<string>(".");
  let unlistenSkills: (() => void) | undefined;

  // Derive cwd the same way the AgentPanel does:
  // current file's parent dir takes priority, fallback to pinned folder.
  function deriveCwd(filePath: string | null, pinned: string | null): string {
    if (filePath) return getBaseDir(filePath);
    if (pinned) return pinned;
    return ".";
  }

  // Track cwd from document + pinned folder stores
  $effect(() => {
    let docPath: string | null = null;
    let pinned: string | null = null;

    const unsubDoc = docStore.subscribe((d) => {
      docPath = d.filePath;
      currentCwd = deriveCwd(docPath, pinned);
    });
    const unsubPin = pinnedFolder.subscribe((p) => {
      pinned = p;
      currentCwd = deriveCwd(docPath, pinned);
    });

    return () => {
      unsubDoc();
      unsubPin();
    };
  });

  // Restart skills watcher when cwd changes
  $effect(() => {
    const cwd = currentCwd;
    if (cwd === ".") {
      stopSkillsWatcher().catch(() => {});
      return;
    }
    startSkillsWatcher(cwd).catch((err) =>
      console.error("[skills] watcher start failed:", err),
    );
    skillsStore.reload(cwd).catch((err) =>
      console.error("[skills] initial reload failed:", err),
    );

    return () => {
      stopSkillsWatcher().catch(() => {});
    };
  });

  // Listen for filesystem changes and refresh on every event
  $effect(() => {
    listenSkillsChanged(() => {
      skillsStore.reload(currentCwd).catch((err) =>
        console.error("[skills] refresh on change failed:", err),
      );
    }).then((fn) => {
      unlistenSkills = fn;
    });

    return () => {
      unlistenSkills?.();
    };
  });

  onMount(async () => {
    await pinnedFolder.load();

    // Restore locale from saved settings
    try {
      const settings = await loadSettings();
      if (settings.locale) {
        locale.set(settings.locale);
      }
    } catch { /* ignore */ }

    // Auto-create the four workspace folders (idempotent — only creates if missing)
    try {
      const dataDir = await getDefaultDataDir();
      const folders = ["pin", "scripts", "sources", "output"];
      for (const name of folders) {
        const fullPath = await joinPath(dataDir, name);
        const exists = await pathExists(fullPath);
        if (!exists) {
          await createFolder(dataDir, name);
        }
      }

      // Write default paths to settings for any folder the user hasn't explicitly set
      const settings = await loadSettings();
      let changed = false;
      if (!settings.pinFolder || !settings.scriptsFolder || !settings.sourcesFolder || !settings.outputFolder) {
        const resolved = await resolveWorkspaceFolders(settings, dataDir);
        if (!settings.pinFolder) { settings.pinFolder = resolved.pinFolder; changed = true; }
        if (!settings.scriptsFolder) { settings.scriptsFolder = resolved.scriptsFolder; changed = true; }
        if (!settings.sourcesFolder) { settings.sourcesFolder = resolved.sourcesFolder; changed = true; }
        if (!settings.outputFolder) { settings.outputFolder = resolved.outputFolder; changed = true; }
      }
      if (changed) {
        await saveSettings(settings);
      }
    } catch {
      // Best-effort — don't block app startup on folder creation failures
    }
  });
</script>

{@render children()}
