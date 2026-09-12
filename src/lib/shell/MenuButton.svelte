<script lang="ts">
  // Title-bar menu (D-19): the File items with accelerators, no native bar.
  export type MenuItem = { id: string; label: string; accel?: string; disabled?: boolean; separatorAfter?: boolean };
  type Props = { items: MenuItem[]; onselect: (id: string) => void };
  let { items, onselect }: Props = $props();

  let open = $state(false);
  let focusIndex = $state(0);
  let root = $state<HTMLElement | null>(null);

  function toggle() {
    open = !open;
    focusIndex = 0;
  }
  function choose(id: string) {
    open = false;
    onselect(id);
  }
  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    const enabled = items.map((it, i) => ({ it, i })).filter(({ it }) => !it.disabled);
    if (e.key === "Escape") {
      open = false;
      e.preventDefault();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const pos = enabled.findIndex(({ i }) => i === focusIndex);
      const next = enabled[(pos + (e.key === "ArrowDown" ? 1 : -1) + enabled.length) % enabled.length];
      if (next) focusIndex = next.i;
      e.preventDefault();
    } else if (e.key === "Enter" || e.key === " ") {
      const it = items[focusIndex];
      if (it && !it.disabled) choose(it.id);
      e.preventDefault();
    }
  }
  function onwindowclick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onwindowclick} />

<div class="menu" bind:this={root} onkeydown={onkeydown} role="presentation">
  <button
    type="button"
    class="trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label="File menu"
    onclick={toggle}>File ▾</button
  >
  {#if open}
    <div class="popup" role="menu" aria-label="File">
      {#each items as it, i (it.id)}
        <button
          type="button"
          role="menuitem"
          class="item"
          class:focused={i === focusIndex}
          disabled={it.disabled}
          tabindex={i === focusIndex ? 0 : -1}
          onmouseenter={() => (focusIndex = i)}
          onclick={() => choose(it.id)}
        >
          <span>{it.label}</span>
          {#if it.accel}<kbd>{it.accel}</kbd>{/if}
        </button>
        {#if it.separatorAfter}<div class="sep" role="separator"></div>{/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
  }
  .trigger {
    all: unset;
    padding: 3px 8px;
    border-radius: 4px;
    color: var(--text-2);
    cursor: default;
    font-size: 12px;
  }
  .trigger:hover,
  .trigger[aria-expanded="true"] {
    background: var(--raised);
    color: var(--text);
  }
  .trigger:focus-visible {
    outline: 2px solid var(--accent);
  }
  .popup {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 240px;
    background: var(--paper);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
    padding: 6px;
    z-index: 60;
    display: grid;
    gap: 1px;
  }
  .item {
    all: unset;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 7px 10px;
    border-radius: 4px;
    cursor: default;
  }
  .item.focused:not(:disabled) {
    background: var(--raised);
  }
  .item:disabled {
    color: var(--disabled);
  }
  .item kbd {
    color: var(--muted);
    border: none;
    background: none;
  }
  .sep {
    height: 1px;
    background: var(--border-soft);
    margin: 4px 2px;
  }
</style>
