<script lang="ts">
  // Page tab (design 4.3, REV-06, M6.8): page state (no matching / no
  // unresolved / approved / approval outdated), facts, issue and region
  // summary, exclusions, deferred items, re-run controls, run history.
  import { api, errorMessage, ISSUE_KIND_LABEL, type PageRow } from "$lib/api";
  import { isTauri } from "$lib/ipc";
  import { KIND_LABEL } from "$lib/stores/layout.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { page: PageRow | null };
  let { page }: Props = $props();

  type Run = { id: string; stage: string; engine: string | null; model: string | null; status: string; elapsed_ms: number | null; started_at: string; error: string | null };
  let runs = $state<Run[]>([]);

  $effect(() => {
    const idx = view.page;
    void page?.status;
    void page?.text_revision;
    if (!isTauri) return;
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

  const onThisPage = $derived(review.page === view.page);
  const open = $derived(onThisPage ? review.issues.filter((i) => i.status === "open") : []);
  const deferred = $derived(onThisPage ? review.issues.filter((i) => i.status === "deferred") : []);
  const resolved = $derived(onThisPage ? review.issues.filter((i) => i.status === "resolved").length : 0);
  const matching = $derived(onThisPage ? review.matching.length : 0);
  const stateLabel = $derived.by(() => {
    if (!page) return "";
    if (page.approval === "current") return "Human approved";
    if (page.approval === "outdated") return "Approval outdated";
    if (page.status !== "done") return page.status;
    if (!page.text_done) return "Not indexed yet";
    if (open.length + deferred.length === 0) return "No unresolved issues";
    if (matching === 0) return "No matching issues";
    return `${matching} matching`;
  });
  const excluded = $derived(onThisPage ? review.regions.filter((r) => r.kind === "ignore" || r.kind === "illustration" || r.kind === "uncertain") : []);
  const regionSummary = $derived.by(() => {
    if (!onThisPage) return "";
    const counts = new Map<string, number>();
    for (const r of review.regions) counts.set(r.kind, (counts.get(r.kind) ?? 0) + 1);
    return [...counts.entries()].map(([k, n]) => `${n} ${KIND_LABEL[k as keyof typeof KIND_LABEL] ?? k}`).join(" · ");
  });
  const gaps = $derived(onThisPage ? review.issues.filter((i) => i.kind === "missing_text" && (i.status === "open" || i.status === "deferred")) : []);
  async function recogniseAll() {
    for (const g of gaps) {
      if (g.bbox) await review.regionOcr(page!.index, g.bbox, {});
    }
  }
  const byKind = $derived.by(() => {
    const m = new Map<string, number>();
    for (const i of open) m.set(i.kind, (m.get(i.kind) ?? 0) + 1);
    return [...m.entries()];
  });

  async function rerunText() {
    if (!page) return;
    try {
      const n = await api.textPassRerun(page.index);
      ui.toast(n ? `Re-running the text pass on page ${page.index + 1}` : "This page is approved or has no layout yet", n ? "ok" : "info", 4000);
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  async function rerunLayout() {
    if (!page) return;
    try {
      await api.layoutRerun(page.index);
      ui.toast(`Re-analysing the layout of page ${page.index + 1}`, "ok");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
</script>

{#if page}
  <div class="row"><span class="muted">Page {page.index + 1}</span><span class="badge" class:ok={page.approval === "current"} class:warn={page.approval === "outdated"}>{stateLabel}</span></div>
  {#if page.approval === "outdated"}
    <p class="note warn">The text or layout changed after approval (revision {page.approved_revision} approved, now {page.text_revision}). Approve again when reviewed.</p>
  {:else if page.approval === "current" && page.approved_outstanding > 0}
    <p class="note">Approved with {page.approved_outstanding} acknowledged outstanding issue{page.approved_outstanding === 1 ? "" : "s"}.</p>
  {/if}
  <div class="facts">
    <div><span class="muted">Size</span><span>{page.width_pt?.toFixed(0)} × {page.height_pt?.toFixed(0)} pt</span></div>
    <div><span class="muted">Printed label</span><span>{page.printed_label ?? "—"}</span></div>
    <div><span class="muted">Status</span><span>{page.status}{page.text_done ? " · indexed" : ""}</span></div>
    {#if regionSummary}<div><span class="muted">Regions</span><span>{regionSummary}</span></div>{/if}
    {#if page.error}<div><span class="muted">Error</span><span class="err">{page.error}</span></div>{/if}
  </div>

  {#if onThisPage && page.text_done}
    <div>
      <div class="label">Issues</div>
      <div class="facts">
        <div><span class="muted">Unresolved</span><span>{open.length} · {matching} below {review.threshold}%</span></div>
        {#each byKind as [k, n] (k)}
          <div class="sub"><span class="muted">{ISSUE_KIND_LABEL[k as keyof typeof ISSUE_KIND_LABEL] ?? k}</span><span>{n}</span></div>
        {/each}
        <div><span class="muted">Deferred</span><span>{deferred.length}</span></div>
        <div><span class="muted">Resolved</span><span>{resolved}</span></div>
      </div>
      {#if deferred.length}
        <ul class="list">
          {#each deferred as d (d.id)}
            <li><button type="button" class="link" onclick={() => { review.deferredView = true; review.select(d.id); }}>{d.original}</button><span class="muted small">deferred</span></li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  {#if gaps.length}
    <div>
      <div class="label">Probable missing text</div>
      <p class="muted small">{gaps.length} area{gaps.length === 1 ? "" : "s"} with ink but no recognised words (LAY-03).</p>
      <div class="two">
        <button type="button" class="ctl" onclick={() => review.select(gaps[0]!.id)}>Review the first</button>
        <button type="button" class="ctl" onclick={recogniseAll} disabled={page.approval === "current"}>Recognise all ×3</button>
      </div>
    </div>
  {/if}
  {#if excluded.length}
    <div>
      <div class="label">Excluded from text</div>
      <ul class="list">
        {#each excluded as r (r.id)}
          <li><span>{KIND_LABEL[r.kind]}</span><span class="muted small">{r.kind === "ignore" ? "not exported" : r.kind === "illustration" ? "image on export" : "needs a decision"}</span></li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="two">
    <button type="button" class="ctl" onclick={rerunText} disabled={!page.layout_done || page.approval === "current"}>Re-run text pass</button>
    <button type="button" class="ctl" onclick={rerunLayout} disabled={!page.ocr_done || page.approval === "current"}>Re-analyse layout</button>
  </div>
  {#if page.approval === "current"}
    <p class="muted small">Reruns are blocked while the page is approved (PIPE-01). Remove the approval first.</p>
  {/if}

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
  .badge.ok {
    background: var(--ok-bg);
    border-color: var(--ok);
  }
  .badge.warn {
    background: var(--accent-bg);
    border-color: var(--warn);
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .note.warn {
    color: var(--accent-text);
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
  .facts > .sub {
    padding-left: 12px;
    font-size: 12px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .err {
    color: var(--danger);
  }
  .label {
    margin-bottom: 4px;
  }
  .list,
  .runs {
    list-style: none;
    padding: 0;
    margin: 4px 0 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .list li,
  .runs li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
  .two {
    display: flex;
    gap: 6px;
  }
  .two .ctl {
    flex: 1;
  }
  .ctl {
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    font: 500 12px var(--font-ui);
    cursor: pointer;
  }
  .ctl:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .link {
    all: unset;
    color: var(--accent-text);
    text-decoration: underline;
    cursor: pointer;
  }
  .link:focus-visible {
    outline: 2px solid var(--accent);
  }
</style>
