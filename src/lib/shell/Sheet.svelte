<script lang="ts">
  // Modal sheet over the workspace (design 4.5). Traps focus, Esc closes.
  import type { Snippet } from "svelte";

  type Props = {
    open: boolean;
    title: string;
    subtitle?: string;
    width?: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  };
  let { open, title, subtitle = "", width = "720px", onclose, children, footer }: Props = $props();

  let panel = $state<HTMLElement | null>(null);

  $effect(() => {
    if (open && panel) {
      const first = panel.querySelector<HTMLElement>(
        "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])",
      );
      (first ?? panel).focus();
    }
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    } else if (e.key === "Tab" && panel) {
      const items = Array.from(
        panel.querySelectorAll<HTMLElement>(
          "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
        ),
      );
      if (items.length === 0) return;
      const first = items[0]!;
      const last = items[items.length - 1]!;
      if (e.shiftKey && document.activeElement === first) {
        last.focus();
        e.preventDefault();
      } else if (!e.shiftKey && document.activeElement === last) {
        first.focus();
        e.preventDefault();
      }
    }
  }
</script>

{#if open}
  <div class="scrim" onclick={onclose} role="presentation"></div>
  <div
    class="sheet"
    style="width:{width}"
    role="dialog"
    aria-modal="true"
    aria-labelledby="sheet-title"
    tabindex="-1"
    bind:this={panel}
    onkeydown={onkeydown}
  >
    <div class="head">
      <span class="title" id="sheet-title">{title}</span>
      {#if subtitle}<span class="sub">{subtitle}</span>{/if}
      <span class="grow"></span>
      <button type="button" class="esc" onclick={onclose} aria-label="Close">Esc</button>
    </div>
    <div class="body">{@render children()}</div>
    {#if footer}<div class="foot">{@render footer()}</div>{/if}
  </div>
{/if}

<style>
  .scrim {
    position: absolute;
    inset: 0;
    background: rgba(42, 38, 34, 0.35);
    backdrop-filter: blur(1.5px) saturate(0.8);
    z-index: 40;
  }
  :global([data-theme="bench"]) .scrim {
    background: rgba(0, 0, 0, 0.55);
  }
  .sheet {
    position: absolute;
    top: 60px;
    left: 50%;
    transform: translateX(-50%);
    max-height: calc(100% - 120px);
    background: var(--paper);
    border-radius: var(--radius-sheet);
    box-shadow: var(--shadow-sheet);
    display: grid;
    grid-template-rows: auto 1fr auto;
    z-index: 41;
    color: var(--text);
  }
  .head {
    padding: 22px 26px 16px;
    border-bottom: 1px solid var(--border-soft);
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .title {
    font-family: var(--font-serif);
    font-size: 24px;
  }
  .sub,
  .esc {
    color: var(--muted);
  }
  .esc {
    all: unset;
    color: var(--muted);
    cursor: default;
  }
  .grow {
    flex: 1;
  }
  .body {
    padding: 20px 26px;
    display: grid;
    gap: 18px;
    overflow: auto;
  }
  .foot {
    padding: 14px 26px;
    border-top: 1px solid var(--border-soft);
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--panel);
    border-radius: 0 0 var(--radius-sheet) var(--radius-sheet);
  }
</style>
