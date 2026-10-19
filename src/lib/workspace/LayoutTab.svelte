<script lang="ts">
  // Layout tab (Pages view): how the book's pages were divided into
  // regions, and which pages need a look in Layout mode: text with no
  // words, uncertain regions, clipping, doubtful reading order.
  import { api, errorMessage, type LayoutPageInfo, type ProjectSummary, type RegionKind } from "$lib/api";
  import { confirmDialog } from "$lib/dialogs";
  import { isTauri } from "$lib/ipc";
  import { exportPolicy, KIND_LABEL } from "$lib/stores/layout.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  const SHOWN = 30;
  let rows = $state<LayoutPageInfo[]>([]);
  let showAll = $state(false);

  $effect(() => {
    void summary.counts.layout_done;
    void summary.counts.text_done;
    void summary.pages;
    if (!isTauri) return;
    api
      .layoutOverview()
      .then((r) => (rows = r))
      .catch(() => (rows = []));
  });

  const inScope = $derived(new Set(summary.pages.filter((p) => p.status !== "unprocessed" && p.status !== "excluded").map((p) => p.index)));
  const scoped = $derived(rows.filter((r) => inScope.has(r.page)));
  const edited = $derived(scoped.filter((r) => r.manual).length);
  const totals = $derived.by(() => {
    const t: Partial<Record<RegionKind, number>> = {};
    for (const r of scoped) for (const [k, n] of Object.entries(r.kinds)) t[k as RegionKind] = (t[k as RegionKind] ?? 0) + (n ?? 0);
    return (Object.entries(t) as [RegionKind, number][]).sort((a, b) => b[1] - a[1]);
  });

  type Check = { page: number; reasons: string[] };
  const checks = $derived.by((): Check[] => {
    const out: Check[] = [];
    for (const r of scoped) {
      const uncertain = r.kinds.uncertain ?? 0;
      const reasons: string[] = [];
      if (r.missing_text) reasons.push(`${r.missing_text} text area${r.missing_text === 1 ? "" : "s"} with no words`);
      if (uncertain) reasons.push(`${uncertain} uncertain region${uncertain === 1 ? "" : "s"}`);
      if (r.clipping) reasons.push("text may be clipped");
      if (r.order) reasons.push("reading order unsure");
      if (reasons.length) out.push({ page: r.page, reasons });
    }
    return out.sort((a, b) => a.page - b.page);
  });
  const shown = $derived(showAll ? checks : checks.slice(0, SHOWN));

  const current = $derived(rows.find((r) => r.page === view.page) ?? null);

  function editLayout(page: number) {
    view.open(page);
    view.editLayout();
  }
  async function reanalyse() {
    const page = view.page;
    if (current?.manual && !(await confirmDialog(`Page ${page + 1} has layout edits made by hand. Re-analysing replaces them with a fresh analysis. Continue?`))) return;
    try {
      await api.layoutRerun(page);
      ui.toast(`Re-analysing the layout of page ${page + 1}`, "info");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
</script>

<div class="head">
  <div class="title">Layout</div>
  <p class="muted small">Divides each page into regions (body, side notes, footnotes, running heads…) that decide reading order and where text goes in Word.</p>
</div>

<div class="card">
  <div class="label">Summary</div>
  <div class="row-between">
    <span>{summary.counts.layout_done} of {summary.counts.in_scope} pages laid out</span>
    <span class="muted">{edited} edited by hand</span>
  </div>
  {#if totals.length}
    <div class="kinds">
      {#each totals as [k, n] (k)}
        <span class="kind {k}">{KIND_LABEL[k] ?? k} <b>{n}</b></span>
      {/each}
    </div>
  {/if}
  <p class="muted small">
    {exportPolicy.native
      ? "In Word: running heads and page numbers go into page headers, footers and unlinked footnotes into page footers, side notes into boxes beside the text."
      : "In Word: running heads, page numbers, footers and unlinked footnotes become styled paragraphs; side notes become boxes beside the text."}
    Linked footnotes always become Word footnotes.
  </p>
</div>

<div class="card">
  <div class="row-between">
    <span class="label">Pages to check</span>
    {#if checks.length}<span class="muted small">{checks.length}</span>{/if}
  </div>
  {#if checks.length}
    <ul class="pages">
      {#each shown as c (c.page)}
        <li>
          <button type="button" class="link" onclick={() => editLayout(c.page)} title="Open page {c.page + 1} in Layout mode">p. {c.page + 1}</button>
          <span class="reasons">{c.reasons.join(" · ")}</span>
        </li>
      {/each}
    </ul>
    {#if checks.length > SHOWN}
      <button type="button" class="link" onclick={() => (showAll = !showAll)}>{showAll ? "Show fewer" : `Show all ${checks.length}`}</button>
    {/if}
    <p class="muted small">Text with no words: draw or fix a region there, then Recognise region. Uncertain: set the region's type.</p>
  {:else}
    <p class="muted small">Nothing flagged. Pages with text the analysis could not place, uncertain regions, clipping, or a doubtful reading order show up here.</p>
  {/if}
</div>

<div class="card">
  <div class="label">This page · p. {view.page + 1}</div>
  {#if current}
    <div class="muted small">{Object.entries(current.kinds).map(([k, n]) => `${n} ${KIND_LABEL[k as RegionKind] ?? k}`).join(", ") || "no regions"}{current.manual ? " · edited by hand" : ""}</div>
  {/if}
  <div class="two">
    <button type="button" class="ctl primary" onclick={() => editLayout(view.page)}>Edit layout <kbd>Ctrl+L</kbd></button>
    <button type="button" class="ctl" onclick={reanalyse} disabled={summary.read_only}>Re-analyse</button>
  </div>
</div>

<style>
  .title {
    font-weight: 600;
    font-size: 14px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
    margin: 4px 0 0;
    line-height: 1.4;
  }
  .card {
    display: grid;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--paper);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .label {
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .row-between {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: baseline;
    gap: 2px 10px;
    font-size: 12.5px;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .kind {
    font-size: 11.5px;
    padding: 2px 7px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--raised);
  }
  .kind.uncertain {
    border-color: var(--warn);
  }
  .kind b {
    font-weight: 600;
  }
  .pages {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 5px;
    font-size: 12.5px;
  }
  .pages li {
    display: grid;
    grid-template-columns: 50px 1fr;
    gap: 8px;
    align-items: baseline;
  }
  .reasons {
    color: var(--text-2);
  }
  .link {
    all: unset;
    color: var(--accent-text);
    cursor: pointer;
    text-decoration: underline;
    font-size: 12.5px;
    justify-self: start;
  }
  .two {
    display: flex;
    gap: 8px;
  }
  .ctl {
    flex: 1;
    padding: 6px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    font: 500 12.5px var(--font-ui);
    cursor: pointer;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
  }
  .ctl.primary {
    background: var(--primary-bg);
    border-color: var(--primary-bg);
    color: var(--primary-fg);
    font-weight: 600;
  }
  .ctl:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ctl kbd {
    font: 500 10px var(--font-mono);
    padding: 0 4px;
    border-radius: 3px;
    border: 1px solid currentColor;
    background: transparent;
    color: inherit;
    opacity: 0.8;
  }
  .ctl:focus-visible,
  .link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
