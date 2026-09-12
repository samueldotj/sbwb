<script lang="ts">
  // Right inspector frame with a tab strip (design section 3 / 4.3).
  import type { Snippet } from "svelte";

  export type InspectorTab = { id: string; label: string; disabled?: boolean };
  type Props = {
    tabs: InspectorTab[];
    active: string;
    onchange?: (id: string) => void;
    width?: string;
    children: Snippet;
    footer?: Snippet;
  };
  let { tabs, active, onchange, width = "var(--inspector-w)", children, footer }: Props = $props();

  function onkeydown(e: KeyboardEvent) {
    const idx = tabs.findIndex((t) => t.id === active);
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      const dir = e.key === "ArrowRight" ? 1 : -1;
      let next = idx;
      for (let i = 0; i < tabs.length; i++) {
        next = (next + dir + tabs.length) % tabs.length;
        if (!tabs[next]?.disabled) break;
      }
      onchange?.(tabs[next]!.id);
      e.preventDefault();
    }
  }
</script>

<aside class="inspector" style="width:{width}">
  <div class="tabs" role="tablist" tabindex="-1" onkeydown={onkeydown}>
    {#each tabs as t (t.id)}
      <button
        type="button"
        role="tab"
        id="tab-{t.id}"
        aria-selected={t.id === active}
        aria-controls="panel-{t.id}"
        tabindex={t.id === active ? 0 : -1}
        class="tab"
        class:active={t.id === active}
        disabled={t.disabled}
        onclick={() => onchange?.(t.id)}>{t.label}</button
      >
    {/each}
  </div>
  <div class="body" role="tabpanel" id="panel-{active}" aria-labelledby="tab-{active}">
    {@render children()}
  </div>
  {#if footer}
    <div class="footer">{@render footer()}</div>
  {/if}
</aside>

<style>
  .inspector {
    background: var(--panel);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: none;
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding: 0 8px;
  }
  .tab {
    all: unset;
    padding: 11px 8px;
    color: var(--muted);
    margin-bottom: -1px;
    border-bottom: 2px solid transparent;
    cursor: default;
  }
  :global([data-theme="bench"]) .tab {
    padding: 9px 8px;
    font-size: 12px;
  }
  .tab.active {
    color: var(--text);
    font-weight: 600;
    border-bottom-color: var(--accent);
  }
  .tab:disabled {
    color: var(--disabled);
  }
  .tab:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .body {
    padding: 14px;
    display: grid;
    gap: 14px;
    align-content: start;
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  .footer {
    padding: 14px;
    border-top: 1px solid var(--border);
    display: grid;
    gap: 8px;
  }
</style>
