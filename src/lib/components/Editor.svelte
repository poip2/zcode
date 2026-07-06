<script lang="ts">
  import { onMount, tick } from "svelte";

  let {
    value,
    onChange,
  }: {
    value: string;
    onChange: (newValue: string) => void;
  } = $props();

  let textareaEl: HTMLTextAreaElement | undefined = $state();
  // svelte-ignore state_referenced_locally
  let localValue = $state(value);

  $effect(() => {
    if (value !== localValue && document.activeElement !== textareaEl) {
      localValue = value;
    }
  });

  onMount(() => {
    tick().then(() => {
      try {
        textareaEl?.focus({ preventScroll: true });
      } catch {
        textareaEl?.focus();
      }
    });
  });

  function handleInput() {
    onChange(localValue);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      const t = e.target as HTMLTextAreaElement;
      const start = t.selectionStart;
      const end = t.selectionEnd;
      const indent = "  ";
      const newValue = t.value.slice(0, start) + indent + t.value.slice(end);
      localValue = newValue;
      onChange(newValue);
      tick().then(() => {
        t.selectionStart = t.selectionEnd = start + indent.length;
      });
    }
  }
</script>

<div class="editor-wrap">
  <textarea
    bind:this={textareaEl}
    bind:value={localValue}
    oninput={handleInput}
    onkeydown={handleKeydown}
    class="editor"
    spellcheck="false"
    autocomplete="off"
    autocapitalize="off"
  ></textarea>
</div>

<style>
  .editor-wrap {
    flex: 1;
    display: flex;
    justify-content: center;
    background: var(--zc-bg-chrome, #F4F2ED);
    min-height: 0;
  }

  .editor {
    width: 100%;
    max-width: 900px;
    height: 100%;
    padding: 32px;
    margin: 0 auto;
    background: transparent;
    border: none;
    outline: none;
    resize: none;
    color: var(--zc-text-primary, #1F1E1C);
    font-family: "SF Mono", "JetBrains Mono", "Fira Code", Menlo, monospace;
    font-size: 14px;
    line-height: 1.6;
    tab-size: 2;
    -moz-tab-size: 2;
    word-wrap: break-word;
    white-space: pre-wrap;
  }

  .editor::placeholder {
    color: var(--zc-text-tertiary, #A8A49D);
  }
</style>
