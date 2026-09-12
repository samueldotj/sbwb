<script lang="ts">
  // Workspace shell (design section 3): rail + mode-specific centre + inspector.
  import PipelineRail, { type StageRow } from "$lib/shell/PipelineRail.svelte";
  import Inspector from "$lib/shell/Inspector.svelte";
  import PageGrid from "$lib/workspace/PageGrid.svelte";
  import ImportInspector from "$lib/workspace/ImportInspector.svelte";
  import PageInspector from "$lib/workspace/PageInspector.svelte";
  import StackedBar from "$lib/workspace/StackedBar.svelte";
  import ScanPane from "$lib/workspace/ScanPane.svelte";
  import Filmstrip from "$lib/workspace/Filmstrip.svelte";
  import TranscriptPane from "$lib/workspace/TranscriptPane.svelte";
  import type { ProjectSummary } from "$lib/api";
  import { renderUrl, THUMB_SCALE } from "$lib/render";
  import { view } from "$lib/stores/view.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import PageMiniGrid from "$lib/workspace/PageMiniGrid.svelte";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  let tab = $state("import");

  $effect(() => {
    if (view.pageCount !== summary.meta.source.page_count) view.reset(summary.meta.source.page_count);
  });
  $effect(() => {
    tab = view.mode === "review" ? "page" : "import";
  });

  const currentPage = $derived(summary.pages[view.page] ?? null);

  const stages = $derived<StageRow[]>([
    { id: "import", label: "Import", state: "done", value: `${summary.counts.in_scope} pp` },
    {
      id: "ocr",
      label: "OCR",
      state: summary.counts.running ? "running" : summary.counts.done >= summary.counts.in_scope && summary.counts.in_scope > 0 ? "done" : "queued",
      value: summary.counts.running || summary.counts.done ? `${summary.counts.done} / ${summary.counts.in_scope}` : "queued",
      progress: summary.counts.in_scope ? summary.counts.done / summary.counts.in_scope : 0,
    },
    { id: "layout", label: "Layout", state: "queued", value: "queued" },
    { id: "text_pass", label: "Text pass", state: "queued", value: "queued" },
    { id: "ai", label: "AI proofread", state: "off", value: "off" },
    { id: "export", label: "Export", state: "off", value: "—" },
  ]);

  const tabs = $derived([
    { id: "import", label: "Import" },
    { id: "page", label: "Page", disabled: view.mode !== "review" },
    { id: "text_pass", label: "Text pass", disabled: true },
    { id: "ai", label: "AI", disabled: true },
  ]);

  function onrail(id: string) {
    if (id === "ai") ui.toast("AI proofreading arrives in a later version.", "info", 4000);
    else if (id === "export") ui.toast("Export arrives in M7.", "info");
    else view.mode = "processing";
  }
</script>

<div class="workspace" class:review={view.mode === "review"} class:strip={view.mode === "review" && ui.theme !== "bench"}>
  <PipelineRail {stages} activeId={view.mode === "processing" ? "import" : ""} onselect={onrail}>
    <div class="review-block">
      <button type="button" class="label linkish" onclick={() => (view.mode = "review")}>Review</button>
      <div class="row"><span>Unresolved</span><b>—</b></div>
      <div class="row"><span>Pages approved</span><b>{summary.counts.approved} / {summary.counts.in_scope}</b></div>
      <StackedBar approved={summary.counts.approved} issues={Math.max(0, summary.counts.done - summary.counts.approved)} unseen={Math.max(0, summary.counts.in_scope - summary.counts.done)} height="6px" />
    </div>
    {#if ui.theme === "bench" && view.mode === "review"}
      <div class="label mini-label">pages</div>
      <PageMiniGrid pages={summary.pages} />
    {/if}
    {#snippet footer()}
      <div class="keys">
        <kbd>PgUp/PgDn</kbd><span>prev / next page</span>
        <kbd>Ctrl +/−</kbd><span>zoom</span>
        <kbd>Esc</kbd><span>pages view</span>
      </div>
    {/snippet}
  </PipelineRail>

  {#if view.mode === "review"}
    {#if ui.theme !== "bench"}
      <Filmstrip pages={summary.pages} />
    {/if}
    <ScanPane pages={summary.pages}>
      {#snippet footer()}
        <span class="chip">Regions</span>
        <span class="chip muted">Edit layout</span>
      {/snippet}
    </ScanPane>
    <TranscriptPane page={currentPage} />
  {:else}
    <section class="center">
      <div class="pane-header">
        <span class="strong">Pages</span>
        <span class="muted">Processing {summary.counts.in_scope} · {summary.counts.unprocessed} untouched in the source</span>
        <span class="grow"></span>
        <span class="muted small">Click a page to open it while OCR continues</span>
      </div>
      <PageGrid
        pages={summary.pages}
        current={view.page}
        onopen={(i) => view.open(i)}
        thumbUrl={(i) => (summary.pages[i]?.status === "unprocessed" ? null : renderUrl(i, THUMB_SCALE))}
      />
    </section>
  {/if}

  <Inspector {tabs} active={tab} onchange={(id) => (tab = id)} width={view.mode === "review" ? "var(--inspector-w)" : "340px"}>
    {#if tab === "page"}
      <PageInspector page={currentPage} />
    {:else}
      <ImportInspector {summary} onstartreview={() => view.open(0)} />
    {/if}
  </Inspector>
</div>

<style>
  .workspace {
    display: grid;
    grid-template-columns: var(--rail-w) 1fr auto;
    min-height: 0;
    height: 100%;
  }
  .workspace.review {
    grid-template-columns: var(--rail-w) minmax(0, 1fr) minmax(0, 1fr) auto;
  }
  .workspace.review.strip {
    grid-template-columns: var(--rail-w) var(--filmstrip-w) minmax(0, 1fr) minmax(0, 1fr) auto;
  }
  .mini-label {
    padding: 8px 6px 6px;
  }
  .center {
    display: grid;
    grid-template-rows: var(--pane-header-h) 1fr;
    min-height: 0;
  }
  .pane-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 16px;
    background: var(--panel-header);
    border-bottom: 1px solid var(--border);
  }
  .strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .grow {
    flex: 1;
  }
  .review-block {
    padding: 4px 6px;
    display: grid;
    gap: 6px;
  }
  .linkish {
    all: unset;
    cursor: default;
  }
  .linkish:hover {
    color: var(--text);
  }
  .row {
    display: flex;
    justify-content: space-between;
  }
  .keys {
    padding: 6px;
    font-size: 11px;
    color: var(--muted);
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 10px;
    align-items: center;
  }
  .chip {
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--paper);
    border: 1px solid var(--border-input);
    font-size: 11px;
  }
</style>
