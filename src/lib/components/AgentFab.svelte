<script lang="ts">
  let {
    open = false,
    onclick,
  }: {
    open: boolean;
    onclick: () => void;
  } = $props();
</script>

<button
  class="agent-fab"
  class:is-open={open}
  onclick={onclick}
  title={open ? "Close AI Agent" : "Open AI Agent"}
>
  {#if open}
    <!-- X icon when open -->
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
      <line x1="18" y1="6" x2="6" y2="18"/>
      <line x1="6" y1="6" x2="18" y2="18"/>
    </svg>
  {:else}
    <!-- Chat bubble with AI dots -->
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
      <circle cx="10" cy="12" r="1.4" fill="currentColor" stroke="none"/>
      <circle cx="14" cy="12" r="1.4" fill="currentColor" stroke="none"/>
      <circle cx="12" cy="7.5" r="1.4" fill="currentColor" stroke="none"/>
    </svg>
  {/if}
</button>

<style>
  .agent-fab {
    position: fixed;
    bottom: 20px;
    right: 20px;
    z-index: 900;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 1px solid var(--zc-border, #E7E4DD);
    background: var(--zc-bg-card, #FDFDFB);
    color: var(--zc-text-primary, #1F1E1C);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.12), 0 0 0 1px rgba(0, 0, 0, 0.04);
    transition: transform 0.15s, box-shadow 0.15s;
  }

  .agent-fab:hover {
    transform: scale(1.06);
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.16), 0 0 0 1px rgba(0, 0, 0, 0.06);
  }

  .agent-fab:active {
    transform: scale(0.96);
  }

  .agent-fab.is-open {
    background: var(--zc-text-primary, #1F1E1C);
    color: var(--zc-bg-card, #FDFDFB);
    border-color: var(--zc-text-primary, #1F1E1C);
  }
</style>
