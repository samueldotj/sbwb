<script lang="ts">
  // Page tab (design 4.3 / REV-06). M2: facts and run history for the page.
  import { api, type PageRow } from "$lib/api";
  import { view } from "$lib/stores/view.svelte";

  type Props = { page: PageRow | null };
  let { page }: Props = $props();

  type Run = { id: string; stage: string; engine: string | null; model: string | null; status: string; elapsed_ms: number | null; started_at: string; error: string | null };
  let runs = $state<Run[]>([]);

  $effect(() => {
    const idx = view.page;
    let cancelled = false;
    api
      .pageRuns(idx)
      .then((r) => {
        if (!cancelled) runs = r as unknown as Run[];
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  const stateLabel = $derived(
    page === null
      ? ""
      : page.approved_revision !== null
        ? "Human approved"
        : page.status === "done"
          ? "No decisions yet"
          : page.status,
  );
</script>

{#if page}
  <div class="row"><span class="muted">Page {page.index + 1}</span><span class="badge">{stateLabel}</span></div>
  <div class="facts">
    <div><span class="muted">Size</span><span>{page.width_pt?.toFixed(0)} × {page.height_pt?.toFixed(0)} pt</span></div>
    <div><span class="muted">Printed label</span><span>{page.printed_label ?? "—"}</span></div>
    <div><span class="muted">Status</span><span>{page.status}</span></div>
    {#if page.error}<div><span class="muted">Error</span><span class="err">{page.error}</span></div>{/if}
  </div>
  <div>
    <div class="label">Runs</div>
    {#if runs.length === 0}
      <p class="muted small">Nothing has run for this page yet.</p>
    {:else}
      <ul class="runs">
        {#each runs as r (r.id)}
          <li>
            <span>{r.stage}{r.model ? ` · ${r.model}` : ""}</span>
            <span class="muted">{r.status}{r.elapsed_ms !== null ? ` · ${(r.elapsed_ms / 1000).toFixed(1)}s` : ""}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .badge {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 10px;
    background: var(--raised);
    border: 1px solid var(--border);
  }
  .facts {
    display: grid;
    gap: 6px;
    font-size: 12.5px;
  }
  .facts > div {
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
    margin: 4px 0 0;
  }
  .err {
    color: var(--danger);
  }
  .label {
    margin-bottom: 4px;
  }
  .runs {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .runs li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
</style>
