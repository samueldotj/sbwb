<script lang="ts">
  // Workspace shell (design section 3). M1: Processing view with the page
  // grid and the import inspector; Review and Layout arrive in M2/M6/M4.
  import PipelineRail, { type StageRow } from "$lib/shell/PipelineRail.svelte";
  import Inspector from "$lib/shell/Inspector.svelte";
  import PageGrid from "$lib/workspace/PageGrid.svelte";
  import ImportInspector from "$lib/workspace/ImportInspector.svelte";
  import StackedBar from "$lib/workspace/StackedBar.svelte";
  import type { ProjectSummary } from "$lib/api";
  import { ui } from "$lib/stores/ui.svelte";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  let currentPage = $state<number | null>(null);
  let tab = $state("import");

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

  const tabs = [
    { id: "import", label: "Import" },
    { id: "page", label: "Page", disabled: true },
    { id: "text_pass", label: "Text pass", disabled: true },
    { id: "ai", label: "AI", disabled: true },
  ];
</script>

<div class="workspace">
  <PipelineRail {stages} activeId="import" onselect={(id) => id === "ai" && ui.toast("AI proofreading arrives in a later version.", "info", 4000)}>
    <div class="review-block">
      <div class="label">Review</div>
      <div class="row"><span>Unresolved</span><b>—</b></div>
      <div class="row"><span>Pages approved</span><b>{summary.counts.approved} / {summary.counts.in_scope}</b></div>
      <StackedBar approved={summary.counts.approved} issues={Math.max(0, summary.counts.done - summary.counts.approved)} unseen={Math.max(0, summary.counts.in_scope - summary.counts.done)} height="6px" />
    </div>
  </PipelineRail>

  <section class="center">
    <div class="pane-header">
      <span class="strong">Pages</span>
      <span class="muted">Processing {summary.counts.in_scope} · {summary.counts.unprocessed} untouched in the source</span>
      <span class="grow"></span>
      <span class="muted small">Click a page to open it while OCR continues</span>
    </div>
    <PageGrid pages={summary.pages} current={currentPage} onopen={(i) => (currentPage = i)} />
  </section>

  <Inspector {tabs} active={tab} onchange={(id) => (tab = id)} width="340px">
    <ImportInspector {summary} onstartreview={() => (currentPage = 0)} />
  </Inspector>
</div>

<style>
  .workspace {
    display: grid;
    grid-template-columns: var(--rail-w) 1fr auto;
    min-height: 0;
    height: 100%;
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
  .review-block .label {
    padding: 0 0 2px;
  }
  .row {
    display: flex;
    justify-content: space-between;
  }
</style>
