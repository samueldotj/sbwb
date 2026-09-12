<script lang="ts">
  // Page grid with status dots (design 4.2). Thumbnails arrive in M2.
  import type { PageRow } from "$lib/api";

  type Props = { pages: PageRow[]; current?: number | null; onopen?: (index: number) => void; thumbUrl?: (index: number) => string | null };
  let { pages, current = null, onopen, thumbUrl }: Props = $props();
</script>

<div class="grid" role="group" aria-label="Pages">
  {#each pages as p (p.index)}
    <button
      type="button"
      class="tile {p.status}"
      class:current={p.index === current}
      aria-label="Page {p.index + 1}, {p.status}{p.error ? `: ${p.error}` : ''}"
      title={p.error ?? undefined}
      onclick={() => onopen?.(p.index)}
    >
      {#if thumbUrl?.(p.index)}
        <img src={thumbUrl(p.index) ?? undefined} alt="" loading="lazy" />
      {/if}
      <span class="dot {p.status}" aria-hidden="true"></span>
      <span class="num">{p.index + 1}</span>
    </button>
  {/each}
</div>

<style>
  .grid {
    padding: 18px;
    display: grid;
    grid-template-columns: repeat(10, 1fr);
    gap: 10px;
    align-content: start;
    overflow: auto;
  }
  .tile {
    all: unset;
    aspect-ratio: 3 / 4;
    background: var(--paper);
    border: 1px solid var(--border-input);
    border-radius: 2px;
    position: relative;
    display: grid;
    place-items: end center;
    padding-bottom: 4px;
    font-size: 10px;
    color: var(--muted);
    cursor: default;
    overflow: hidden;
  }
  .tile img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .tile.unprocessed,
  .tile.excluded {
    background: var(--strip);
    border-style: dashed;
    color: var(--disabled);
  }
  .tile.current {
    border: 2px solid var(--accent);
    color: var(--accent);
    font-weight: 700;
  }
  .tile:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .num {
    position: relative;
    z-index: 1;
    padding: 0 4px;
    background: var(--paper);
    border-radius: 2px;
  }
  .dot {
    position: absolute;
    right: 3px;
    top: 3px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    z-index: 1;
  }
  .dot.done {
    background: var(--ok);
  }
  .dot.failed {
    background: var(--danger);
  }
  .dot.queued {
    border: 1px solid var(--disabled);
  }
  .dot.running {
    width: 7px;
    height: 7px;
    border: 2px solid var(--accent);
    border-right-color: transparent;
    animation: spin 0.9s linear infinite;
  }
  .dot.unprocessed,
  .dot.excluded {
    display: none;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
