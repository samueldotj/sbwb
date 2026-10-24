<script lang="ts">
  // Workspace shell (design section 3): rail + mode-specific centre + inspector.
  import { untrack } from "svelte";
  import PipelineRail, { type StageRow } from "$lib/shell/PipelineRail.svelte";
  import Inspector from "$lib/shell/Inspector.svelte";
  import PageGrid from "$lib/workspace/PageGrid.svelte";
  import ImportInspector from "$lib/workspace/ImportInspector.svelte";
  import PageInspector from "$lib/workspace/PageInspector.svelte";
  import TextPassInspector from "$lib/workspace/TextPassInspector.svelte";
  import OcrInspector from "$lib/workspace/OcrInspector.svelte";
  import LayoutTab from "$lib/workspace/LayoutTab.svelte";
  import IssueInspector from "$lib/workspace/IssueInspector.svelte";
  import WordHighlights from "$lib/workspace/WordHighlights.svelte";
  import GroupSheet from "$lib/workspace/GroupSheet.svelte";
  import ExportSheet from "$lib/workspace/ExportSheet.svelte";
  import AiTab from "$lib/workspace/AiTab.svelte";
  import { review } from "$lib/stores/review.svelte";
  import StackedBar from "$lib/workspace/StackedBar.svelte";
  import ScanPane from "$lib/workspace/ScanPane.svelte";
  import Filmstrip from "$lib/workspace/Filmstrip.svelte";
  import TranscriptPane from "$lib/workspace/TranscriptPane.svelte";
  import type { ProjectSummary } from "$lib/api";
  import { pickScale, renderUrl, THUMB_SCALE } from "$lib/render";
  import { PAGE_TILE_MAX, PAGE_TILE_MIN } from "$lib/stores/ui.svelte";
  import { previews } from "$lib/stores/previews.svelte";
  import { view } from "$lib/stores/view.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import PageMiniGrid from "$lib/workspace/PageMiniGrid.svelte";
  import Sheet from "$lib/shell/Sheet.svelte";
  import ChoiceDialog from "$lib/shell/ChoiceDialog.svelte";
  import RegionOverlay from "$lib/workspace/RegionOverlay.svelte";
  import ReadingOrder from "$lib/workspace/ReadingOrder.svelte";
  import RegionInspector from "$lib/workspace/RegionInspector.svelte";
  import { layout, exportPolicy } from "$lib/stores/layout.svelte";
  import { confirmDialog } from "$lib/dialogs";
  import type { OcrWord } from "$lib/api";
  import { formatEta, pipeline } from "$lib/stores/pipeline.svelte";
  import { api, errorMessage, type ExportEntry, type ProcessingSettings } from "$lib/api";
  import { isTauri } from "$lib/ipc";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  let tab = $state("import");
  let aiEnabled = $state(false);
  // Word documents exported from this book (PRJ-01: the rail shows the
  // outputs; clicking opens the file with its default app).
  let exports = $state<ExportEntry[]>([]);
  $effect(() => {
    void summary.counts;
    if (!isTauri) return;
    api.exportsList().then((e) => (exports = e)).catch(() => (exports = []));
  });
  async function openExport(e: ExportEntry) {
    try {
      await api.openFile(e.path);
    } catch (err) {
      ui.toast(errorMessage(err), "error", 6000);
      api.exportsList().then((x) => (exports = x)).catch(() => {});
    }
  }
  $effect(() => {
    void tab;
    try {
      aiEnabled = !!JSON.parse(localStorage.getItem("sbwb.ai.prefs") ?? "{}").enabled;
    } catch {
      aiEnabled = false;
    }
  });
  let issueTab = $state<IssueInspector | null>(null);
  let group = $state<{ original: string; replacement: string } | null>(null);
  $effect(() => {
    if (ui.inspectorTab) {
      tab = ui.inspectorTab;
      ui.inspectorTab = null;
    }
  });
  let logOpen = $state(false);
  const logTime = (ts: string) => {
    const d = new Date(ts);
    return Number.isNaN(d.getTime()) ? ts : d.toLocaleTimeString();
  };
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

  /** A tab asked for together with a mode change (a rail click). */
  let nextTab: string | null = null;

  // ----- where the book was left (per viewer, per book) -----
  // Opening a book returns to the mode, page, inspector tab and scan zoom it
  // had when it was closed. Kept in local storage under the book's id: a
  // convenience of this viewer, not part of the project file.
  type SavedView = { mode: typeof view.mode; page: number; tab: string; zoomMode: typeof view.zoomMode; zoom: number };
  const VALID_TABS: Record<typeof view.mode, string[]> = {
    processing: ["import", "ocr", "layout", "text_pass", "ai"],
    review: ["issue", "page", "text_pass", "ai"],
    layout: ["region", "page"],
  };
  const viewKey = (id: string) => `sbwb.view.${id}`;
  function readView(id: string): SavedView | null {
    try {
      const v = JSON.parse(localStorage.getItem(viewKey(id)) ?? "null") as SavedView | null;
      return v && typeof v.page === "number" && v.mode in VALID_TABS ? v : null;
    } catch {
      return null;
    }
  }
  let restoredFor: string | null = null;
  $effect(() => {
    const id = summary.meta.id;
    const pages = summary.meta.source.page_count;
    if (id === restoredFor && view.pageCount === pages) return;
    restoredFor = id;
    untrack(() => {
      view.reset(pages);
      const saved = readView(id);
      if (!saved) return;
      view.goTo(saved.page);
      view.zoomMode = saved.zoomMode ?? "fit";
      view.zoom = saved.zoom ?? 1;
      const want = VALID_TABS[saved.mode].includes(saved.tab) ? saved.tab : null;
      nextTab = want;
      view.mode = saved.mode;
      if (want) tab = want;
    });
  });
  $effect(() => {
    const saved: SavedView = { mode: view.mode, page: view.page, tab, zoomMode: view.zoomMode, zoom: view.zoom };
    const id = restoredFor;
    if (!id || view.pageCount === 0) return;
    try {
      localStorage.setItem(viewKey(id), JSON.stringify(saved));
    } catch {
      /* storage unavailable */
    }
  });
  $effect(() => {
    const fallback = view.mode === "review" ? "issue" : view.mode === "layout" ? "region" : "import";
    tab = nextTab ?? fallback;
    nextTab = null;
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
  // The dirty check is untracked: reading it subscribes to the regions, so
  // every finished load (and every edit) would start another load that
  // overwrites the edit.
  // An autosave bumps the revision; the store already holds that version.
  $effect(() => {
    const idx = view.page;
    const rev = currentPage?.layout_revision;
    if (view.mode === "processing") return;
    untrack(() => {
      if (layout.page === idx && (layout.dirty || layout.saving || rev === layout.revision)) return;
      void layout.load(idx);
    });
  });
  $effect(() => {
    const idx = view.page;
    void layout.wordsRevision;
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
    const page = currentPage.index;
    // Issues the reviewer can see must be dealt with or acknowledged; those
    // above the threshold were never shown and are acknowledged silently.
    const { open, shown } = review.page === page ? review.pageOutstanding : { open: [], shown: [] };
    const hidden = open.length - shown.length;
    if (shown.length > 0) {
      const words = shown.map((i) => `“${i.original}”`);
      const list = words.length > 6 ? `${words.slice(0, 6).join(", ")} and ${words.length - 6} more` : words.join(", ");
      approveMessage = `${shown.length} issue${shown.length === 1 ? " is" : "s are"} still open on page ${page + 1}: ${list}.${hidden ? ` ${hidden} more scored ${review.threshold}% or above and are not shown; they are recorded as acknowledged too.` : ""} Approving records them as acknowledged.`;
      const choice = await new Promise<string>((resolve) => {
        approveResolve = resolve;
        approveDialog = true;
      });
      approveDialog = false;
      if (choice === "show") {
        review.select(shown[0]!.id);
        return;
      }
      if (choice !== "approve") return;
    }
    await review.approve(page, open.length);
    if (shown.length === 0 && hidden > 0) {
      ui.toast(`Page ${page + 1} approved · ${hidden} issue${hidden === 1 ? "" : "s"} above your ${review.threshold}% threshold recorded as acknowledged`, "ok", 5000);
    }
  }
  let approveDialog = $state(false);
  let approveMessage = $state("");
  let approveResolve: ((id: string) => void) | null = null;

  async function leaveLayout() {
    await layout.flush();
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
      } else if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "z") {
        void review.undoLatest();
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
        void layout.flush();
        e.preventDefault();
      } else if ((e.key === "Delete" || e.key === "Backspace") && !e.defaultPrevented) {
        // A focused region handles its own Delete; this covers the selection.
        layout.removeMany(layout.selection);
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
  // Pages recognised in the whole book, like the Layout and Text pass rows;
  // a run's own count restarts when processing resumes, so it goes in the
  // detail line instead.
  const ocrValue = $derived.by(() => {
    if (pipeline.state === "paused") return "paused";
    if (summary.counts.ocr_done > 0 || summary.counts.failed > 0) return `${summary.counts.ocr_done} / ${summary.counts.in_scope}`;
    return "queued";
  });
  // Speed, time left and failures under the OCR row (the former job monitor).
  const ocrDetail = $derived.by(() => {
    const o = pipeline.ocr;
    const parts: string[] = [];
    if (o && pipeline.active) {
      parts.push(`this run ${o.done + o.failed} / ${o.total}`);
      if (o.secs_per_unit) parts.push(`${o.secs_per_unit.toFixed(1)} s/page`);
      if (o.eta_ms !== null && pipeline.state === "running") parts.push(formatEta(o.eta_ms));
      if (o.failed) parts.push(`${o.failed} failed`);
    } else if (summary.counts.failed) {
      parts.push(`${summary.counts.failed} failed`);
    }
    return parts.join(" · ") || undefined;
  });
  const stages = $derived<StageRow[]>([
    { id: "import", label: "Import", state: "done", value: `${summary.counts.in_scope} pp` },
    {
      id: "ocr",
      label: "OCR",
      state: ocrState,
      value: ocrValue,
      progress: summary.counts.in_scope ? summary.counts.ocr_done / summary.counts.in_scope : 0,
      detail: ocrDetail,
      detailTitle: settingsLine,
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
    { id: "ai", label: "AI proofread", state: aiEnabled ? "queued" : "off", value: aiEnabled ? "on request" : "off" },
    { id: "export", label: "Export", state: exports.some((e) => e.exists) ? "done" : summary.counts.text_done > 0 ? "queued" : "off", value: exports.filter((e) => e.exists).length ? `${exports.filter((e) => e.exists).length} file${exports.filter((e) => e.exists).length === 1 ? "" : "s"}` : summary.counts.text_done > 0 ? "ready" : "—" },
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
            { id: "ai", label: "AI" },
          ]
        : [
            { id: "import", label: "Import" },
            { id: "ocr", label: "OCR" },
            { id: "layout", label: "Layout" },
            { id: "page", label: "Page", disabled: true },
            { id: "text_pass", label: "Text pass" },
            { id: "ai", label: "AI" },
          ],
  );

  /** Render scale for a thumbnail at the current size: sharp, never larger than needed. */
  function thumbScale(i: number): number {
    const w = summary.pages[i]?.width_pt ?? 400;
    return Math.max(THUMB_SCALE, pickScale(ui.pageTile / w, window.devicePixelRatio || 1));
  }

  /** Every page the grid shows a thumbnail for, at the scale it asks for. */
  function previewItems(): { page: number; scale: number }[] {
    return summary.pages.filter((p) => p.status !== "unprocessed").map((p) => ({ page: p.index, scale: thumbScale(p.index) }));
  }
  // Generate all thumbnails in the background while the Pages view is open:
  // on entering it, and again (after a pause) when the size or range
  // changes. A new size also lifts a Stop.
  let previewTile = 0;
  let previewTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const open = view.mode === "processing";
    const tile = ui.pageTile;
    void summary.counts.in_scope;
    if (!open) return;
    untrack(() => {
      if (tile !== previewTile) {
        if (previewTile) previews.stopped = false;
        previewTile = tile;
      }
    });
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(() => void previews.start(previewItems()), 400);
    return () => {
      if (previewTimer) clearTimeout(previewTimer);
    };
  });

  function onrail(id: string) {
    if (id === "ai") tab = "ai";
    else if (id === "export") ui.exportOpen = true;
    else if (id === "text_pass") tab = "text_pass";
    else {
      const want = id === "ocr" || id === "layout" ? id : "import";
      if (view.mode === "layout") void layout.flush();
      if (view.mode === "processing") tab = want;
      else {
        nextTab = want;
        view.mode = "processing";
      }
    }
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="workspace" class:review={view.mode === "review"} class:layoutmode={view.mode === "layout"} class:strip={view.mode === "review" && ui.theme !== "bench"}>
  <PipelineRail {stages} activeId={view.mode === "processing" ? (tab === "ocr" || tab === "layout" ? tab : "import") : ""} onselect={onrail}>
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
    {#if exports.length}
      <div class="exports-block">
        <div class="label">Word output</div>
        {#each exports.slice(0, 4) as e (e.id)}
          <div class="file" class:missing={!e.exists}>
            <button type="button" class="doc" onclick={() => openExport(e)} disabled={!e.exists} title={e.exists ? `Open ${e.path}` : `${e.path} is no longer there`}>
              <span class="icon" aria-hidden="true">W</span>
              <span class="fname">{e.name}</span>
            </button>
            <div class="fmeta">
              <span>{e.kind} · {e.pages} pp</span>
              {#if e.exists}<button type="button" class="link" onclick={() => api.openPath(e.path)} aria-label="Show {e.name} in its folder">folder</button>{/if}
              {#if e.archive}<button type="button" class="link" onclick={() => api.openPath(e.archive!)} aria-label="Show the archive bundle for {e.name}">archive</button>{/if}
            </div>
          </div>
        {/each}
        {#if exports.length > 4}<div class="muted small">{exports.length - 4} older in the Export sheet</div>{/if}
      </div>
    {/if}
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
        </div>
        <div class="log-preview" aria-label="Latest pipeline activity">
          {#each pipeline.log.slice(0, 3) as l, i (i)}
            <div class="logline {l.level}" title="{logTime(l.ts)} {l.text}">{l.text}</div>
          {:else}
            <div class="logline muted">No activity yet.</div>
          {/each}
          <button type="button" class="ctl muted" onclick={() => (logOpen = true)}>Log{pipeline.log.length ? ` (${pipeline.log.length})` : ""}</button>
        </div>
      {/if}
      <div class="keys">
        {#if view.mode === "review"}
          <kbd>J / K</kbd><span>next / prev issue</span>
          <kbd>A E S L</kbd><span>accept · edit · skip · later</span>
          <kbd>Ctrl+Z</kbd><span>undo</span>
        {/if}
        <kbd>PgUp/PgDn</kbd><span>prev / next page</span>
        <kbd>Ctrl +/−</kbd><span>zoom</span>
        <kbd>Esc</kbd><span>pages view</span>
      </div>
    {/snippet}
  </PipelineRail>
  <ChoiceDialog
    open={approveDialog}
    title="Approve page {view.page + 1}?"
    message={approveMessage}
    choices={[{ id: "show", label: "Show them", kind: "primary" }, { id: "approve", label: "Approve anyway" }, { id: "cancel", label: "Cancel" }]}
    onchoose={(id) => approveResolve?.(id)}
  />
  <Sheet open={logOpen} title="Pipeline log" subtitle="{pipeline.state} · {settingsLine}" width="680px" onclose={() => (logOpen = false)}>
    <div class="log" role="log" aria-live="polite">
      {#each pipeline.log as l, i (i)}
        <div class="logline {l.level}"><span class="muted">{logTime(l.ts)}</span> {l.text}</div>
      {:else}
        <div class="muted">Nothing yet.</div>
      {/each}
    </div>
  </Sheet>

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
        <button type="button" class="tool" onclick={() => layout.removeMany(layout.selection)} disabled={layout.selection.length === 0} title="Delete the selected regions (Del)" aria-label="Delete selected regions (Del)">Delete{layout.selection.length > 1 ? ` ${layout.selection.length}` : ""}<span class="key">Del</span></button>
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
        <span class="muted small hint">Click a page to open it while OCR continues</span>
        <span class="previews" aria-live="polite">
          {#if previews.total && previews.done >= previews.total}
            <span class="muted small">Previews ready</span>
          {:else if previews.total}
            <span class="muted small">Previews {previews.done} / {previews.total}</span>
            {#if previews.running}
              <button type="button" class="zbtn" onclick={() => previews.stop()}>Stop</button>
            {:else}
              <button type="button" class="zbtn" onclick={() => previews.start(previewItems(), true)}>Generate</button>
            {/if}
          {/if}
        </span>
        <label class="tile-size" title="Thumbnail size (Ctrl+wheel over the pages)">
          <button type="button" class="zbtn" onclick={() => ui.zoomPageTiles(-1)} disabled={ui.pageTile <= PAGE_TILE_MIN} aria-label="Smaller thumbnails">−</button>
          <input type="range" min={PAGE_TILE_MIN} max={PAGE_TILE_MAX} step="5" bind:value={ui.pageTile} aria-label="Thumbnail size" />
          <button type="button" class="zbtn" onclick={() => ui.zoomPageTiles(1)} disabled={ui.pageTile >= PAGE_TILE_MAX} aria-label="Larger thumbnails">+</button>
        </label>
      </div>
      <PageGrid
        pages={summary.pages}
        current={view.page}
        onopen={(i) => view.open(i)}
        tile={ui.pageTile}
        onzoom={(d) => ui.zoomPageTiles(d)}
        thumbUrl={(i) => (summary.pages[i]?.status === "unprocessed" ? null : renderUrl(i, thumbScale(i)))}
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
    {:else if tab === "ocr"}
      <OcrInspector {summary} />
    {:else if tab === "layout"}
      <LayoutTab {summary} />
    {:else if tab === "ai"}
      <AiTab {summary} />
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
          <button type="button" class="ctl" onclick={() => layout.undo()} disabled={!layout.canUndo} title="Undo (Ctrl+Z)">Undo <kbd>Ctrl+Z</kbd></button>
          <span class="muted small autosave" aria-live="polite">{layout.saving ? "Saving…" : layout.dirty ? "Changes pending" : "All changes saved"}</span>
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
  .tool:disabled {
    opacity: 0.4;
  }
  .tool:focus-visible {
    outline: 2px solid var(--accent);
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .autosave {
    align-self: center;
    text-align: center;
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
  .pane-header .hint {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .previews {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .tile-size {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .tile-size input {
    width: 110px;
    accent-color: var(--accent);
  }
  .zbtn {
    all: unset;
    padding: 1px 7px;
    border-radius: 5px;
    border: 1px solid var(--border-input);
    background: var(--paper);
    line-height: 1.2;
    cursor: default;
  }
  .zbtn:disabled {
    opacity: 0.4;
  }
  .zbtn:focus-visible {
    outline: 2px solid var(--accent);
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
  .exports-block {
    padding: 10px 14px 4px;
    border-top: 1px solid var(--border-soft);
    display: grid;
    gap: 6px;
  }
  .file {
    display: grid;
    gap: 2px;
  }
  .file.missing {
    opacity: 0.55;
  }
  .doc {
    all: unset;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 6px;
    margin: 0 -6px;
    border-radius: 5px;
    cursor: pointer;
    min-width: 0;
  }
  .doc:hover:not(:disabled) {
    background: var(--raised);
  }
  .doc:focus-visible {
    outline: 2px solid var(--accent);
  }
  .doc:disabled {
    cursor: default;
  }
  .icon {
    flex: none;
    width: 16px;
    height: 18px;
    border-radius: 3px;
    background: #2b579a;
    color: #fff;
    font: 700 10px var(--font-ui);
    display: grid;
    place-items: center;
  }
  .fname {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-decoration: underline;
    text-decoration-color: var(--border-input);
  }
  .fmeta {
    display: flex;
    gap: 8px;
    font-size: 11px;
    color: var(--muted);
    padding-left: 23px;
  }
  .fmeta .link {
    all: unset;
    color: var(--accent-text);
    cursor: pointer;
    text-decoration: underline;
  }
  .fmeta .link:focus-visible {
    outline: 2px solid var(--accent);
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
  .log-preview {
    padding: 0 6px 6px;
    display: grid;
    gap: 2px;
    font-size: 11px;
    color: var(--text-2);
  }
  .log-preview .logline {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .logline.warn {
    color: var(--warn);
  }
  .logline.error {
    color: var(--danger);
  }
  .log {
    font: 12px var(--font-mono);
    display: grid;
    gap: 3px;
    max-height: 60vh;
    overflow: auto;
  }
  .log .logline {
    overflow-wrap: anywhere;
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
