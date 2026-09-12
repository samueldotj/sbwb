<script lang="ts">
  // Workspace shell (design section 3): rail + mode-specific centre + inspector.
  import PipelineRail, { type StageRow } from "$lib/shell/PipelineRail.svelte";
  import Inspector from "$lib/shell/Inspector.svelte";
  import PageGrid from "$lib/workspace/PageGrid.svelte";
  import ImportInspector from "$lib/workspace/ImportInspector.svelte";
  import PageInspector from "$lib/workspace/PageInspector.svelte";
  import TextPassInspector from "$lib/workspace/TextPassInspector.svelte";
  import IssueInspector from "$lib/workspace/IssueInspector.svelte";
  import WordHighlights from "$lib/workspace/WordHighlights.svelte";
  import GroupSheet from "$lib/workspace/GroupSheet.svelte";
  import ExportSheet from "$lib/workspace/ExportSheet.svelte";
  import { review } from "$lib/stores/review.svelte";
  import StackedBar from "$lib/workspace/StackedBar.svelte";
  import ScanPane from "$lib/workspace/ScanPane.svelte";
  import Filmstrip from "$lib/workspace/Filmstrip.svelte";
  import TranscriptPane from "$lib/workspace/TranscriptPane.svelte";
  import type { ProjectSummary } from "$lib/api";
  import { renderUrl, THUMB_SCALE } from "$lib/render";
  import { view } from "$lib/stores/view.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import PageMiniGrid from "$lib/workspace/PageMiniGrid.svelte";
  import JobMonitor from "$lib/workspace/JobMonitor.svelte";
  import RegionOverlay from "$lib/workspace/RegionOverlay.svelte";
  import ReadingOrder from "$lib/workspace/ReadingOrder.svelte";
  import RegionInspector from "$lib/workspace/RegionInspector.svelte";
  import { layout, exportPolicy } from "$lib/stores/layout.svelte";
  import { confirmDialog } from "$lib/dialogs";
  import type { OcrWord } from "$lib/api";
  import { pipeline } from "$lib/stores/pipeline.svelte";
  import { api, type ProcessingSettings } from "$lib/api";
  import { isTauri } from "$lib/ipc";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  let tab = $state("import");
  let issueTab = $state<IssueInspector | null>(null);
  let group = $state<{ original: string; replacement: string } | null>(null);
  $effect(() => {
    if (ui.inspectorTab) {
      tab = ui.inspectorTab;
      ui.inspectorTab = null;
    }
  });
  let monitorOpen = $state(false);
  let settings = $state<ProcessingSettings | null>(null);
  $effect(() => {
    if (isTauri) api.settingsGet().then((s) => (settings = s)).catch(() => {});
  });
  $effect(() => {
    if (!isTauri) return;
    void summary.meta.settings;
    api.exportDefaultsGet().then((d) => (exportPolicy.native = d.furniture === "native_headers_footers")).catch(() => {});
  });
  const settingsLine = $derived(settings ? `${settings.model === "eng_best" ? "eng best" : "eng fast"} · ${settings.dpi}dpi · deskew ${settings.deskew ? "on" : "off"}` : "");

  $effect(() => {
    if (view.pageCount !== summary.meta.source.page_count) view.reset(summary.meta.source.page_count);
  });
  $effect(() => {
    tab = view.mode === "review" ? "issue" : view.mode === "layout" ? "region" : "import";
  });
  // Review data follows the page and its text revision (decisions, reruns).
  $effect(() => {
    const idx = view.page;
    void currentPage?.text_revision;
    void currentPage?.text_done;
    if (view.mode === "processing") return;
    void review.loadPrefs().then(() => review.load(idx));
  });
  $effect(() => {
    if (view.mode === "review") void review.refreshCounts();
  });

  const currentPage = $derived(summary.pages[view.page] ?? null);
  let pageWords = $state<OcrWord[]>([]);

  // Regions for the current page follow the page and its layout revision.
  $effect(() => {
    const idx = view.page;
    void currentPage?.layout_revision;
    if (view.mode === "processing") return;
    if (layout.dirty && layout.page === idx) return;
    void layout.load(idx);
  });
  $effect(() => {
    const idx = view.page;
    if (view.mode !== "layout" || !isTauri) {
      pageWords = [];
      return;
    }
    api.pageOcr(idx).then((o) => (pageWords = o?.words ?? [])).catch(() => (pageWords = []));
  });

  async function approveCurrent() {
    if (!currentPage || !currentPage.text_done) return;
    if (currentPage.approval === "current") {
      const ok = await confirmDialog(`Remove the approval of page ${currentPage.index + 1}?`, "Approval");
      if (ok) await review.unapprove(currentPage.index);
      return;
    }
    const outstanding = review.page === currentPage.index ? review.pageUnresolved : 0;
    if (outstanding > 0) {
      const ok = await confirmDialog(`${outstanding} issue${outstanding === 1 ? " is" : "s are"} still unresolved or deferred on page ${currentPage.index + 1}. Approve anyway and record them as acknowledged?`, "Approve page");
      if (!ok) return;
    }
    await review.approve(currentPage.index, outstanding);
  }

  async function leaveLayout() {
    if (layout.dirty) {
      const ok = await confirmDialog("Discard the unsaved layout changes on this page?", "Layout");
      if (!ok) return;
      layout.revert();
    }
    view.mode = "review";
  }
  function onkeydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement | null)?.closest("input, textarea, [contenteditable]");
    if (typing) return;
    if (view.mode === "review" && (e.key === "l" || e.key === "L") && (e.ctrlKey || e.metaKey)) {
      view.editLayout();
      e.preventDefault();
    } else if (view.mode === "review" && group === null) {
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        void approveCurrent();
        e.preventDefault();
      } else if (e.ctrlKey || e.metaKey || e.altKey) {
        return;
      } else if (e.key === "j" || e.key === "J") {
        void review.step(true);
        e.preventDefault();
      } else if (e.key === "k" || e.key === "K") {
        void review.step(false);
        e.preventDefault();
      } else if (tab === "issue" && issueTab?.onkey(e)) {
        e.preventDefault();
      }
    } else if (view.mode === "layout") {
      if (e.key === "Escape") {
        void leaveLayout();
        e.preventDefault();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        layout.undo();
        e.preventDefault();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        void layout.save();
        e.preventDefault();
      } else if (!e.ctrlKey && !e.metaKey && !e.altKey) {
        const k = e.key.toLowerCase();
        if (k === "v") layout.tool = "select";
        else if (k === "r") layout.tool = "draw";
        else if (k === "x") layout.tool = "split";
        else if (k === "m") layout.tool = "merge";
        else return;
        e.preventDefault();
      }
    }
  }

  const ocrState = $derived.by((): StageRow["state"] => {
    if (pipeline.state === "running" || pipeline.state === "stopping" || pipeline.state === "paused") return "running";
    if (summary.counts.failed > 0 && summary.counts.queued === 0) return "failed";
    if (summary.counts.in_scope > 0 && summary.counts.done >= summary.counts.in_scope) return "done";
    return "queued";
  });
  const ocrValue = $derived.by(() => {
    const o = pipeline.ocr;
    if (pipeline.state === "paused") return "paused";
    if (o && pipeline.active) return `${o.done + o.failed} / ${o.total}`;
    if (summary.counts.done > 0 || summary.counts.failed > 0) return `${summary.counts.done} / ${summary.counts.in_scope}`;
    return "queued";
  });
  const stages = $derived<StageRow[]>([
    { id: "import", label: "Import", state: "done", value: `${summary.counts.in_scope} pp` },
    {
      id: "ocr",
      label: "OCR",
      state: ocrState,
      value: ocrValue,
      progress: pipeline.ocr && pipeline.ocr.total ? (pipeline.ocr.done + pipeline.ocr.failed) / pipeline.ocr.total : summary.counts.in_scope ? summary.counts.done / summary.counts.in_scope : 0,
    },
    {
      id: "layout",
      label: "Layout",
      state: view.mode === "layout" ? "running" : summary.counts.layout_done >= summary.counts.in_scope && summary.counts.in_scope > 0 ? "done" : summary.counts.layout_done > 0 ? "running" : "queued",
      value: view.mode === "layout" ? "editing" : summary.counts.layout_done > 0 ? `${summary.counts.layout_done} / ${summary.counts.in_scope}` : "queued",
      progress: summary.counts.in_scope ? summary.counts.layout_done / summary.counts.in_scope : 0,
    },
    {
      id: "text_pass",
      label: "Text pass",
      state: pipeline.text && pipeline.text.running > 0 ? "running" : summary.counts.text_done >= summary.counts.in_scope && summary.counts.in_scope > 0 ? "done" : summary.counts.text_done > 0 ? "running" : "queued",
      value: summary.counts.text_done > 0 ? `${summary.counts.text_done} / ${summary.counts.in_scope}` : "queued",
      progress: summary.counts.in_scope ? summary.counts.text_done / summary.counts.in_scope : 0,
    },
    { id: "ai", label: "AI proofread", state: "off", value: "off" },
    { id: "export", label: "Export", state: summary.counts.text_done > 0 ? "queued" : "off", value: summary.counts.approved > 0 ? `${summary.counts.approved} approved` : summary.counts.text_done > 0 ? "ready" : "—" },
  ]);

  const tabs = $derived(
    view.mode === "layout"
      ? [
          { id: "region", label: "Region" },
          { id: "page", label: "Page" },
        ]
      : view.mode === "review"
        ? [
            { id: "issue", label: "Issue" },
            { id: "page", label: "Page" },
            { id: "text_pass", label: "Text pass" },
            { id: "ai", label: "AI", disabled: true },
          ]
        : [
            { id: "import", label: "Import" },
            { id: "page", label: "Page", disabled: true },
            { id: "text_pass", label: "Text pass" },
            { id: "ai", label: "AI", disabled: true },
          ],
  );

  function onrail(id: string) {
    if (id === "ai") ui.toast("AI proofreading arrives in a later version.", "info", 4000);
    else if (id === "export") ui.exportOpen = true;
    else if (id === "text_pass") tab = "text_pass";
    else {
      view.mode = "processing";
      if (id === "ocr") monitorOpen = true;
    }
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="workspace" class:review={view.mode === "review"} class:layoutmode={view.mode === "layout"} class:strip={view.mode === "review" && ui.theme !== "bench"} class:monitor={view.mode === "processing" && monitorOpen}>
  <PipelineRail {stages} activeId={view.mode === "processing" ? "import" : ""} onselect={onrail}>
    <div class="review-block">
      <button type="button" class="label linkish" onclick={() => (view.mode = "review")}>Review</button>
      <div class="row"><span>Unresolved</span><b>{review.counts.unresolved || "—"}</b></div>
      <div class="row"><span>Below {review.threshold}%</span><b>{review.counts.matching}</b></div>
      {#if review.counts.deferred}<div class="row"><span>Deferred</span><b>{review.counts.deferred}</b></div>{/if}
      <div class="row"><span>Pages approved</span><b>{summary.counts.approved} / {summary.counts.in_scope}</b></div>
      <StackedBar approved={summary.counts.approved} issues={Math.max(0, summary.counts.text_done - summary.counts.approved)} unseen={Math.max(0, summary.counts.in_scope - summary.counts.text_done)} height="6px" />
      {#if view.mode === "review"}
        <div class="filters">
          <label><input type="checkbox" bind:checked={review.deferredView} onchange={() => { review.selectedId = null; void review.refreshCounts(); }} /> Deferred view</label>
          <label><input type="checkbox" bind:checked={review.byPriority} onchange={() => review.savePrefs()} /> By priority</label>
          <label><input type="checkbox" bind:checked={review.hideCompleted} onchange={() => review.savePrefs()} /> Hide accepted</label>
          <label><input type="checkbox" bind:checked={review.hideAuto} onchange={() => review.savePrefs()} /> Hide auto-applied</label>
        </div>
      {/if}
    </div>
    {#if ui.theme === "bench" && view.mode === "review"}
      <div class="label mini-label">pages</div>
      <PageMiniGrid pages={summary.pages} />
    {/if}
    {#snippet footer()}
      {#if view.mode === "processing"}
        <div class="controls">
          {#if pipeline.state === "running" || pipeline.state === "stopping"}
            <button type="button" class="ctl" onclick={() => pipeline.pause()} disabled={pipeline.state === "stopping"}>Pause all</button>
          {:else if pipeline.state === "paused"}
            <button type="button" class="ctl primary" onclick={() => pipeline.resume()}>Resume</button>
            <button type="button" class="ctl" onclick={() => pipeline.cancel()}>Stop</button>
          {:else if summary.counts.failed > 0}
            <button type="button" class="ctl primary" onclick={() => pipeline.retryFailed()}>Retry {summary.counts.failed} failed</button>
            {#if summary.counts.queued > 0}<button type="button" class="ctl" onclick={() => pipeline.start()}>Process {summary.counts.queued} queued</button>{/if}
          {:else if summary.counts.queued > 0 && !summary.read_only}
            <button type="button" class="ctl primary" onclick={() => pipeline.start()}>Process {summary.counts.queued} pages</button>
          {/if}
          {#if !monitorOpen}<button type="button" class="ctl muted" onclick={() => (monitorOpen = true)}>Details</button>{/if}
        </div>
      {/if}
      <div class="keys">
        {#if view.mode === "review"}
          <kbd>J / K</kbd><span>next / prev issue</span>
          <kbd>A E S L</kbd><span>accept · edit · skip · later</span>
        {/if}
        <kbd>PgUp/PgDn</kbd><span>prev / next page</span>
        <kbd>Ctrl +/−</kbd><span>zoom</span>
        <kbd>Esc</kbd><span>pages view</span>
      </div>
    {/snippet}
  </PipelineRail>
  {#if view.mode === "processing" && monitorOpen}
    <JobMonitor {summary} {settingsLine} oncollapse={() => (monitorOpen = false)} />
  {/if}

  {#if view.mode === "review"}
    {#if ui.theme !== "bench"}
      <Filmstrip pages={summary.pages} />
    {/if}
    <ScanPane pages={summary.pages}>
      {#snippet overlay({ page, zoom })}
        {#if layout.showRegions && layout.page === page}
          <RegionOverlay regions={layout.regions} {zoom} pageW={currentPage?.width_pt ?? 612} pageH={currentPage?.height_pt ?? 792} />
        {/if}
        {#if review.page === page}
          <WordHighlights {zoom} />
        {/if}
      {/snippet}
      {#snippet footer()}
        <button type="button" class="chip" class:on={layout.showRegions} onclick={() => (layout.showRegions = !layout.showRegions)}>Regions</button>
        <button type="button" class="chip" onclick={() => view.editLayout()} disabled={!currentPage?.ocr_done}>Edit layout <kbd>Ctrl+L</kbd></button>
      {/snippet}
    </ScanPane>
    <TranscriptPane page={currentPage} />
  {:else if view.mode === "layout"}
    <ScanPane pages={summary.pages} overlayInteractive={true}>
      {#snippet tools()}
        {#each [["select", "Select", "V"], ["draw", "Draw", "R"], ["split", "Split", "X"], ["merge", "Merge", "M"]] as [id, label, key] (id)}
          <button type="button" class="tool" class:on={layout.tool === id} onclick={() => (layout.tool = id as typeof layout.tool)} aria-pressed={layout.tool === id} title="{label} ({key})" aria-label="{label} ({key})">{label}<span class="key">{key}</span></button>
        {/each}
        <span class="muted small">{String(layout.report?.columns ?? 1)} col</span>
      {/snippet}
      {#snippet overlay({ page, zoom })}
        {#if layout.page === page}
          <RegionOverlay regions={layout.regions} {zoom} interactive={true} pageW={currentPage?.width_pt ?? 612} pageH={currentPage?.height_pt ?? 792} />
        {/if}
      {/snippet}
    </ScanPane>
    <ReadingOrder words={pageWords} />
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

  <Inspector {tabs} active={tab} onchange={(id) => (tab = id)} width={view.mode === "review" ? "var(--inspector-w)" : view.mode === "layout" ? "264px" : "340px"}>
    {#if tab === "region"}
      <RegionInspector pageW={currentPage?.width_pt ?? 612} pageH={currentPage?.height_pt ?? 792} words={pageWords} />
    {:else if tab === "issue"}
      <IssueInspector bind:this={issueTab} page={currentPage} ongroup={(original, replacement) => (group = { original, replacement })} />
    {:else if tab === "page"}
      <PageInspector page={currentPage} />
    {:else if tab === "text_pass"}
      <TextPassInspector {summary} page={currentPage} />
    {:else}
      <ImportInspector {summary} onstartreview={() => view.open(0)} eta={pipeline.ocr?.eta_ms ?? null} secsPerPage={pipeline.ocr?.secs_per_unit ?? null} pipelineState={pipeline.state} />
    {/if}
    {#snippet footer()}
      {#if view.mode === "review" && currentPage}
        <div class="approve">
          <span class="muted small">Page {currentPage.index + 1} · {review.page === currentPage.index ? review.pageUnresolved : "–"} unresolved</span>
          <button type="button" class="ctl" class:primary={currentPage.approval !== "current"} onclick={approveCurrent} disabled={!currentPage.text_done} aria-label={currentPage.approval === "current" ? "Remove approval" : "Approve page (Ctrl+Enter)"}>
            {currentPage.approval === "current" ? "Approved ✓" : currentPage.approval === "outdated" ? "Approve again" : "Approve page"} <kbd>Ctrl+↵</kbd>
          </button>
        </div>
      {/if}
      {#if view.mode === "layout"}
        <div class="two">
          <button type="button" class="ctl" onclick={() => layout.revert()} disabled={!layout.dirty}>Revert</button>
          <button type="button" class="ctl primary" onclick={() => layout.save()} disabled={!layout.dirty}>Save layout</button>
        </div>
        <div class="two">
          <button type="button" class="ctl muted" onclick={() => layout.rerun()}>Re-analyse page</button>
          <button type="button" class="ctl muted" onclick={leaveLayout}>Done <kbd>Esc</kbd></button>
        </div>
      {/if}
    {/snippet}
  </Inspector>
  {#if group}
    <GroupSheet open={group !== null} original={group.original} replacement={group.replacement} onclose={() => (group = null)} />
  {/if}
  {#if ui.exportOpen}
    <ExportSheet open={ui.exportOpen} onclose={() => (ui.exportOpen = false)} />
  {/if}
</div>

<style>
  .workspace {
    display: grid;
    grid-template-columns: var(--rail-w) 1fr auto;
    min-height: 0;
    height: 100%;
  }
  .workspace.monitor {
    grid-template-columns: var(--rail-w) 360px 1fr auto;
  }
  .workspace.review {
    grid-template-columns: var(--rail-w) minmax(0, 1fr) minmax(0, 1fr) auto;
  }
  .workspace.layoutmode {
    grid-template-columns: var(--rail-w) minmax(0, 1fr) 320px auto;
  }
  .tool {
    all: unset;
    padding: 3px 7px;
    border-radius: 5px;
    border: 1px solid var(--border-input);
    background: var(--paper);
    font-size: 12px;
    cursor: default;
  }
  .tool.on {
    background: var(--primary-bg);
    color: var(--primary-fg);
    border-color: var(--primary-bg);
  }
  .tool .key {
    opacity: 0.6;
    margin-left: 4px;
    font-size: 10px;
  }
  @container (max-width: 520px) {
    .tool .key {
      display: none;
    }
  }
  .tool:focus-visible {
    outline: 2px solid var(--accent);
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
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
  .filters {
    display: grid;
    gap: 3px;
    margin-top: 8px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .filters label {
    display: flex;
    gap: 6px;
    align-items: center;
    min-height: 24px; /* 24 px targets (WCAG 2.2 2.5.8) */
  }
  .approve {
    display: grid;
    gap: 6px;
  }
  .approve .ctl {
    display: inline-flex;
    justify-content: center;
    gap: 6px;
  }
  .approve .ctl kbd {
    font: 500 10.5px var(--font-mono);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid currentColor;
    background: transparent;
    color: inherit;
    opacity: 0.8;
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
  .controls {
    padding: 6px;
    display: grid;
    gap: 6px;
  }
  .ctl {
    all: unset;
    text-align: center;
    padding: 7px;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-input);
    background: var(--paper);
    font-size: 12px;
    cursor: default;
  }
  .ctl.primary {
    background: var(--primary-bg);
    color: var(--primary-fg);
    border-color: var(--primary-bg);
    font-weight: 600;
  }
  .ctl.muted {
    color: var(--muted);
    border-color: transparent;
    background: none;
  }
  .ctl:disabled {
    opacity: 0.5;
  }
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
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
    all: unset;
    cursor: default;
    padding: 3px 8px;
    border-radius: 5px;
    background: var(--paper);
    border: 1px solid var(--border-input);
    font-size: 11px;
  }
  .chip.on {
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .chip:disabled {
    opacity: 0.5;
  }
  .chip:focus-visible {
    outline: 2px solid var(--accent);
  }
  .chip kbd {
    font-size: 10px;
    margin-left: 4px;
  }
</style>
