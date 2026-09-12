<script lang="ts">
  // Vertical page filmstrip (design 4.3): thumbnails with status dots, the
  // current page enlarged. Keyboard: arrows move, Enter opens.
  import type { PageRow } from "$lib/api";
  import { renderUrl, THUMB_SCALE } from "$lib/render";
  import { view } from "$lib/stores/view.svelte";

  type Props = { pages: PageRow[] };
  let { pages }: Props = $props();

  let list = $state<HTMLElement | null>(null);

  $effect(() => {
    const el = list?.querySelector<HTMLElement>(`[data-page="${view.page}"]`);
    el?.scrollIntoView({ block: "nearest" });
  });

  function dot(p: PageRow): string {
    if (p.approved_revision !== null) return "approved";
    if (p.status === "failed") return "failed";
    if (p.status === "done") return "issues";
    if (p.status === "unprocessed" || p.status === "excluded") return "none";
    return "unseen";
  }
</script>

<nav class="strip" aria-label="Pages" bind:this={list}>
  {#each pages as p (p.index)}
    <button
      type="button"
      class="thumb"
      class:current={p.index === view.page}
      class:out={p.status === "unprocessed" || p.status === "excluded"}
      data-page={p.index}
      aria-label="Page {p.index + 1}"
      aria-current={p.index === view.page ? "page" : undefined}
      onclick={() => view.goTo(p.index)}
    >
      {#if p.status !== "unprocessed" && p.status !== "excluded"}
        <img src={renderUrl(p.index, THUMB_SCALE)} alt="" loading="lazy" draggable="false" />
      {/if}
      <span class="dot {dot(p)}" aria-hidden="true"></span>
      <span class="num">{p.index + 1}</span>
    </button>
  {/each}
  <div class="legend" aria-hidden="true">
    <span><i class="dot approved"></i> approved</span>
    <span><i class="dot issues"></i> issues</span>
    <span><i class="dot unseen"></i> unseen</span>
  </div>
</nav>

<style>
  .strip {
    width: var(--filmstrip-w);
    background: var(--strip);
    border-right: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
    padding: 10px 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    min-height: 0;
  }
  .thumb {
    all: unset;
    width: 44px;
    height: 58px;
    background: var(--paper);
    border: 1px solid var(--border-input);
    border-radius: 2px;
    position: relative;
    flex: none;
    cursor: default;
    overflow: hidden;
  }
  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb.out {
    background: var(--track);
    border-style: dashed;
  }
  .thumb.current {
    width: 48px;
    height: 62px;
    border: 2px solid var(--accent);
  }
  .thumb:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .num {
    position: absolute;
    bottom: 2px;
    left: 0;
    right: 0;
    text-align: center;
    font-size: 9px;
    color: var(--muted);
    background: rgba(251, 248, 242, 0.85);
  }
  .current .num {
    font-weight: 700;
    color: var(--accent);
  }
  .dot {
    position: absolute;
    right: -1px;
    top: -1px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    display: inline-block;
  }
  .legend .dot {
    position: static;
  }
  .dot.approved {
    background: var(--ok);
  }
  .dot.issues {
    background: var(--warn);
  }
  .dot.unseen {
    background: var(--unseen);
  }
  .dot.failed {
    background: var(--danger);
  }
  .dot.none {
    display: none;
  }
  .legend {
    margin-top: auto;
    font-size: 10px;
    color: var(--muted);
    text-align: center;
    line-height: 1.5;
    display: grid;
  }
</style>
