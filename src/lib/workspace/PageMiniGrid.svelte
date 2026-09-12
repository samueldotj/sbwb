<script lang="ts">
  // Bench rail page mini-grid (design 4.3): one square per page, coloured by
  // state, the current page outlined. Replaces the filmstrip in Bench.
  import type { PageRow } from "$lib/api";
  import { view } from "$lib/stores/view.svelte";

  type Props = { pages: PageRow[] };
  let { pages }: Props = $props();

  function cls(p: PageRow): string {
    if (p.approved_revision !== null) return "approved";
    if (p.status === "failed") return "failed";
    if (p.status === "done") return "issues";
    if (p.status === "unprocessed" || p.status === "excluded") return "out";
    return "unseen";
  }
</script>

<div class="mini" role="group" aria-label="Pages">
  {#each pages as p (p.index)}
    <button
      type="button"
      class="cell {cls(p)}"
      class:current={p.index === view.page}
      aria-label="Page {p.index + 1}"
      aria-current={p.index === view.page ? "page" : undefined}
      onclick={() => view.goTo(p.index)}
    ></button>
  {/each}
</div>

<style>
  .mini {
    padding: 0 6px;
    display: grid;
    grid-template-columns: repeat(10, 1fr);
    gap: 3px;
    overflow: auto;
    max-height: 180px;
  }
  .cell {
    all: unset;
    aspect-ratio: 1;
    border-radius: 2px;
    background: var(--track);
    cursor: default;
  }
  .cell.approved {
    background: #3b7a5a;
  }
  .cell.issues {
    background: #b08a3a;
  }
  .cell.failed {
    background: var(--danger);
  }
  .cell.out {
    background: var(--track);
    opacity: 0.4;
  }
  .cell.current {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .cell:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
