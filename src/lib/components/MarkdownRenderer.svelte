<script lang="ts">
  import { onMount, tick } from "svelte";
  import { tt, locale } from "$lib/i18n";

  let { html = "" }: { html: string } = $props();

  let articleEl: HTMLElement | undefined = $state();

  $effect(() => {
    html;
    const _ = $locale;
    tick().then(() => {
      if (!articleEl) return;
      addCodeCopyButtons();
    });
  });

  function addCodeCopyButtons() {
    if (!articleEl) return;
    const pres = articleEl.querySelectorAll("pre");
    for (const pre of pres) {
      let btn = pre.querySelector(".code-copy-btn") as HTMLButtonElement | null;
      if (!btn) {
        (pre as HTMLElement).style.position = "relative";
        btn = document.createElement("button");
        btn.className = "code-copy-btn";
        btn.addEventListener("click", () => {
          const code = pre.querySelector("code");
          const text = code?.textContent ?? pre.textContent ?? "";
          navigator.clipboard.writeText(text).then(() => {
            btn!.textContent = tt('markdown.copied');
            setTimeout(() => (btn!.textContent = tt('markdown.copy')), 1500);
          });
        });
        pre.appendChild(btn);
      }
      btn.textContent = tt('markdown.copy');
    }
  }
</script>

<article
  bind:this={articleEl}
  class="md-content prose prose-slate max-w-none mx-auto px-8 py-8"
  style="max-width: min(100%, 900px); font-size: 17px; line-height: 1.7;"
>
  {@html html}
</article>

<style>
  .md-content {
    color: var(--zc-text-primary, #1F1E1C);
  }

  article :global(h1) {
    font-size: 1.75em;
    font-weight: 700;
    letter-spacing: -0.02em;
    margin-top: 1.5em;
    color: var(--zc-text-primary, #1F1E1C);
  }

  article :global(h2) {
    font-size: 1.4em;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--zc-text-primary, #1F1E1C);
  }

  article :global(h3) {
    font-size: 1.15em;
    font-weight: 600;
    color: var(--zc-text-primary, #1F1E1C);
  }

  article :global(pre) {
    border-radius: 10px;
    padding: 1em 1.2em;
    overflow-x: auto;
    font-size: 0.8em;
    border: 1px solid var(--zc-border, #E7E4DD);
    background: #f6f8fa !important;
    color: #24292f !important;
  }

  article :global(code) {
    font-family: "SF Mono", "JetBrains Mono", "Fira Code", Menlo, monospace;
  }

  article :global(:not(pre) > code) {
    background: #f2f2f7;
    padding: 0.15em 0.4em;
    border-radius: 5px;
    font-size: 0.85em;
    color: var(--zc-text-secondary, #8A8782);
  }

  article :global(:not(pre) > code)::before,
  article :global(:not(pre) > code)::after {
    content: none;
  }

  article :global(table) {
    border-collapse: collapse;
    width: 100%;
    overflow-x: auto;
    display: block;
    font-size: 0.9em;
  }

  article :global(th),
  article :global(td) {
    border: 1px solid var(--zc-border, #E7E4DD);
    padding: 0.5em 0.75em;
    text-align: left;
  }

  article :global(th) {
    background: #f2f2f7;
    font-weight: 600;
    font-size: 0.9em;
    color: var(--zc-text-secondary, #8A8782);
  }

  article :global(blockquote) {
    border-left: 3px solid var(--zc-text-secondary, #8A8782);
    padding-left: 1em;
    margin-left: 0;
    color: var(--zc-text-secondary, #8A8782);
  }

  article :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 8px;
  }

  article :global(hr) {
    border: none;
    border-top: 1px solid var(--zc-border, #E7E4DD);
    margin: 2em 0;
  }

  article :global(.task-list-item) {
    list-style: none;
    margin-left: -1.5em;
  }

  article :global(.task-list-item input[type="checkbox"]) {
    margin-right: 0.5em;
    accent-color: var(--zc-text-secondary, #8A8782);
  }

  article :global(a) {
    color: var(--zc-text-secondary, #8A8782);
    text-decoration: none;
  }

  article :global(a:hover) {
    text-decoration: underline;
  }

  /* Code copy button */
  article :global(.code-copy-btn) {
    position: absolute;
    top: 8px;
    right: 8px;
    padding: 3px 10px;
    font-size: 11px;
    font-weight: 500;
    font-family: -apple-system, sans-serif;
    color: #8e8e93;
    background: rgba(255,255,255,0.8);
    border: 1px solid #e5e5ea;
    border-radius: 5px;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s, background 0.15s;
    backdrop-filter: blur(4px);
  }

  article :global(pre:hover .code-copy-btn) {
    opacity: 1;
  }

  article :global(.code-copy-btn:hover) {
    background: rgba(255,255,255,0.95);
    color: #1c1c1e;
  }

  /* KaTeX */
  article :global(.katex-display) {
    overflow-x: auto;
    padding: 0.5em 0;
  }

  article :global(.katex) {
    font-size: 1.1em;
  }
</style>
