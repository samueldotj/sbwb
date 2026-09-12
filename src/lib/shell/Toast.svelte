<script lang="ts">
  // Concise, non-disruptive feedback (UX-04). Reads from the ui store.
  import { ui } from "$lib/stores/ui.svelte";
</script>

<div class="toasts" aria-live="polite" aria-relevant="additions">
  {#each ui.toasts as t (t.id)}
    <div class="toast {t.kind}" role="status">
      <span>{t.text}</span>
      <button type="button" class="x" aria-label="Dismiss" onclick={() => ui.dismiss(t.id)}>✕</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: absolute;
    left: 50%;
    bottom: calc(var(--statusbar-h) + 14px);
    transform: translateX(-50%);
    display: grid;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: var(--radius-control);
    background: var(--text);
    color: var(--bg);
    font-size: 12.5px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }
  .toast.ok {
    border-left: 3px solid var(--ok);
  }
  .toast.warn {
    border-left: 3px solid var(--warn);
  }
  .toast.error {
    border-left: 3px solid var(--danger);
  }
  .x {
    all: unset;
    opacity: 0.6;
    cursor: default;
  }
  .x:hover {
    opacity: 1;
  }
</style>
