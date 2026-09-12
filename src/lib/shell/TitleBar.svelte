<script lang="ts">
  // Custom title bar (design section 3, D-19, D-20). Drag region is the bar
  // itself; the three window controls call the Tauri window API.
  import { isTauri } from "$lib/ipc";
  import { ui } from "$lib/stores/ui.svelte";

  import type { Snippet } from "svelte";

  type Props = {
    context?: string;
    bookTitle?: string;
    bookMeta?: string;
    menu_?: Snippet;
  };
  let { context = "", bookTitle = "", bookMeta = "", menu_ }: Props = $props();

  async function win() {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return getCurrentWindow();
  }
  async function minimize() {
    if (isTauri) (await win()).minimize();
  }
  async function toggleMaximize() {
    if (isTauri) (await win()).toggleMaximize();
  }
  async function close() {
    if (isTauri) (await win()).close();
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="left" data-tauri-drag-region>
    <span class="mark" aria-hidden="true">S</span>
    <span class="name">SBWB</span>
    {#if menu_}{@render menu_()}{/if}
    {#if context}<span class="context">{context}</span>{/if}
  </div>
  <div class="center" data-tauri-drag-region>
    {#if bookTitle}
      <span class="book">{bookTitle}</span>
      {#if bookMeta}<span class="meta">{bookMeta}</span>{/if}
    {/if}
  </div>
  <div class="right">
    <button
      class="ctl"
      type="button"
      title="Switch theme"
      aria-label="Switch theme"
      onclick={() => ui.toggleTheme()}>◐</button
    >
    <button class="ctl" type="button" aria-label="Minimize" onclick={minimize}>—</button>
    <button class="ctl" type="button" aria-label="Maximize" onclick={toggleMaximize}>▢</button>
    <button class="ctl close" type="button" aria-label="Close" onclick={close}>✕</button>
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-h);
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    padding: 0 6px 0 14px;
    background: var(--chrome);
    border-bottom: 1px solid var(--border);
    color: var(--text);
  }
  .left,
  .center,
  .right {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .center {
    justify-content: center;
    font-family: var(--font-serif);
    font-size: 15px;
    white-space: nowrap;
  }
  .right {
    justify-content: flex-end;
    gap: 2px;
  }
  .mark {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    background: var(--primary-bg);
    color: var(--primary-fg);
    font: 600 11px var(--font-serif);
    display: grid;
    place-items: center;
  }
  :global([data-theme="bench"]) .mark {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    font: 700 10px var(--font-mono);
  }
  .name {
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .context,
  .meta {
    color: var(--muted);
    font-family: var(--font-ui);
    font-size: 12px;
  }
  :global([data-theme="bench"]) .meta {
    font-family: var(--font-mono);
    font-size: 10.5px;
  }
  .ctl {
    all: unset;
    cursor: default;
    color: var(--muted);
    width: 36px;
    height: calc(var(--titlebar-h) - 6px);
    display: grid;
    place-items: center;
    border-radius: 4px;
  }
  .ctl:hover {
    background: var(--raised);
    color: var(--text);
  }
  .ctl.close:hover {
    background: var(--danger);
    color: #fff;
  }
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
  }
</style>
