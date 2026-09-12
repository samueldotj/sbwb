<script lang="ts">
  // Text pass tab (design 4.6, TXT-03): threshold slider, counters,
  // Re-run pass, Review N →, last-run line. Book-wide; the current page's
  // own proposals are listed underneath so the pass is inspectable before
  // the review loop (M6) exists.
  import { api, errorMessage, type PageRow, type ProcessingSettings, type ProjectSummary, type StoredProposal } from "$lib/api";
  import { isTauri } from "$lib/ipc";
  import { textPass } from "$lib/stores/text.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { summary: ProjectSummary; page: PageRow | null };
  let { summary, page }: Props = $props();

  let settings = $state<ProcessingSettings | null>(null);
  let threshold = $state(90);
  let rerunPages = $state<number | null>(null);
  let busy = $state(false);
  let proposals = $state<StoredProposal[]>([]);

  $effect(() => {
    if (!isTauri) return;
    api
      .settingsGet()
      .then((s) => {
        settings = s;
        threshold = s.auto_apply_threshold;
      })
      .catch(() => {});
  });

  // Preview how many pages a changed threshold would rerun (PIPE-01).
  $effect(() => {
    if (!settings || threshold === settings.auto_apply_threshold) {
      rerunPages = null;
      return;
    }
    const t = threshold;
    const s = settings;
    let cancelled = false;
    api
      .settingsPreview({ ...s, auto_apply_threshold: t })
      .then((p) => {
        if (!cancelled) rerunPages = p.rerun_pages;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const idx = view.page;
    void page?.text_revision;
    void textPass.summary;
    if (!isTauri || !page?.text_done) {
      proposals = [];
      return;
    }
    let cancelled = false;
    api
      .pageText(idx)
      .then((t) => {
        if (!cancelled) proposals = t?.proposals ?? [];
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  const dirty = $derived(settings !== null && threshold !== settings.auto_apply_threshold);
  const s = $derived(textPass.summary);

  async function applyThreshold() {
    if (!settings) return;
    busy = true;
    try {
      const next = { ...settings, auto_apply_threshold: threshold };
      const n = await api.settingsSet(next, true);
      settings = next;
      ui.toast(n > 0 ? `Threshold saved · re-running the text pass on ${n} pages` : "Threshold saved", "ok");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    } finally {
      busy = false;
    }
  }

  async function rerun(all: boolean) {
    busy = true;
    try {
      const n = await api.textPassRerun(all ? undefined : view.page);
      ui.toast(n > 0 ? `Re-running the text pass on ${n} page${n === 1 ? "" : "s"}` : "Nothing to re-run: no page has a layout yet, or every page is approved", n > 0 ? "ok" : "info", 5000);
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    } finally {
      busy = false;
    }
  }

  const KIND: Record<string, string> = { hyphen_join: "join", ocr_confusion: "confusion", spelling: "spelling", proper_name: "name" };
  function when(iso: string | null): string {
    if (!iso) return "never";
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
  }
</script>

<div class="head">
  <div class="title">Text pass</div>
  <p class="muted small">Joins line-broken words, rebuilds paragraphs, and proposes corrections against the dictionaries. Every change keeps its source words.</p>
</div>

<div class="control">
  <label class="name" for="tp-threshold">Auto-apply at or above</label>
  <div class="slider">
    <input id="tp-threshold" type="range" min="50" max="100" bind:value={threshold} disabled={!settings || busy} />
    <span class="mono">{threshold}%</span>
  </div>
  <p class="muted small">Below this, changes stay as suggestions. Human-edited or approved text is never touched.</p>
  {#if dirty}
    <div class="two">
      <button type="button" class="ctl" onclick={() => (threshold = settings?.auto_apply_threshold ?? 90)} disabled={busy}>Revert</button>
      <button type="button" class="ctl primary" onclick={applyThreshold} disabled={busy}>
        Apply{rerunPages !== null ? ` · re-run ${rerunPages} pages` : ""}
      </button>
    </div>
  {/if}
</div>

<div class="counters" aria-label="Text pass counters">
  <div><b>{s?.applied ?? 0}</b><span class="muted">applied</span></div>
  <div><b>{s?.suggested ?? 0}</b><span class="muted">suggested</span></div>
  <div><b>{s ? s.rejected + s.stale : 0}</b><span class="muted">undone</span></div>
</div>

<div class="two">
  <button type="button" class="ctl" onclick={() => rerun(true)} disabled={busy || summary.counts.layout_done === 0}>Re-run pass</button>
  <button type="button" class="ctl primary" onclick={() => ui.toast("The review loop arrives with M6.", "info")} disabled={!s || s.suggested === 0}>Review {s?.suggested ?? 0} →</button>
</div>
<p class="muted small">Last run {when(s?.last_run ?? null)} · {s?.pages_done ?? 0} of {s?.pages_in_scope ?? summary.counts.in_scope} pages</p>

{#if page}
  <div class="pagelist">
    <div class="label">This page</div>
    {#if !page.text_done}
      <p class="muted small">{page.layout_done ? "The text pass has not run on this page yet." : "Runs after OCR and layout."}</p>
    {:else if proposals.length === 0}
      <p class="muted small">No changes proposed on page {page.index + 1}.</p>
    {:else}
      <ul>
        {#each proposals as p (p.id)}
          <li class={p.status}>
            <span class="kind">{KIND[p.kind] ?? p.kind}</span>
            <span class="change"><s>{p.original}</s> → <b>{p.replacement}</b></span>
            <span class="score mono">{p.score}</span>
            <span class="status">{p.status === "applied_auto" ? "applied" : p.status}</span>
          </li>
        {/each}
      </ul>
      <div class="two">
        <button type="button" class="ctl" onclick={() => rerun(false)} disabled={busy || page.approved_revision !== null}>Re-run this page</button>
      </div>
    {/if}
  </div>
{/if}

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
  .control {
    display: grid;
    gap: 6px;
  }
  .name {
    font-size: 12.5px;
    font-weight: 500;
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .slider input {
    flex: 1;
    accent-color: var(--accent);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 12px;
    min-width: 34px;
    text-align: right;
  }
  .counters {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .counters > div {
    display: grid;
    justify-items: center;
    padding: 8px 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
  }
  .counters b {
    font-size: 18px;
    font-weight: 600;
  }
  .counters span {
    font-size: 11px;
  }
  .two {
    display: flex;
    gap: 8px;
  }
  .two .ctl {
    flex: 1;
  }
  .ctl {
    padding: 6px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    font: 500 12.5px var(--font-ui);
    cursor: pointer;
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
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .label {
    margin-bottom: 4px;
  }
  .pagelist ul {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .pagelist li {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: 8px;
    align-items: baseline;
    padding: 4px 6px;
    border-radius: 4px;
    background: var(--raised);
  }
  .kind {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .change {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .status {
    font-size: 11px;
    color: var(--muted);
  }
  li.applied_auto .status {
    color: var(--ok);
  }
  li.open .status {
    color: var(--accent-text);
  }
</style>
