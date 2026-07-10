<script lang="ts">
  import { onMount } from "svelte";
  import { pinnedFolder } from "$lib/stores/pinnedFolder";
  import { document as docStore } from "$lib/stores/document";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import { startSkillsWatcher, stopSkillsWatcher, listenSkillsChanged } from "$lib/tauri/watcher";
  import { getBaseDir } from "$lib/tauri/files";
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

  onMount(() => {
    pinnedFolder.load();
  });
</script>

{@render children()}
