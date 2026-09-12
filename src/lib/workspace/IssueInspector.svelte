<script lang="ts">
  // Issue tab (design 4.3, REV-03, REV-05, REV-06): evidence crop, current
  // reading, score chip, reason, numbered candidates, Accept / Edit / Skip /
  // Later, auto-advance, history with undo. After an accepted spelling
  // correction the grouped-correction offer appears (REV-05).
  import { api, errorMessage, ISSUE_KIND_LABEL, type HistoryEntry, type PageRow } from "$lib/api";
  import { isTauri } from "$lib/ipc";
  import { pickScale, renderUrl } from "$lib/render";
  import { review } from "$lib/stores/review.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { page: PageRow | null; ongroup: (original: string, replacement: string) => void };
  let { page, ongroup }: Props = $props();

  const issue = $derived(review.selected);
  const span = $derived(issue ? review.spanById(issue.span) : null);
  const position = $derived(review.selectedIndex);
  let chosen = $state<number>(0);
  let editText = $state("");
  let editOpen = $state(false);
  let editBox = $state<HTMLInputElement | null>(null);
  let history = $state<HistoryEntry[]>([]);
  let draftTimer: ReturnType<typeof setTimeout> | null = null;

  // Reset per issue; restore a persisted draft when one exists (PRJ-04).
  $effect(() => {
    const id = issue?.id;
    chosen = 0;
    editOpen = false;
    editText = issue?.original ?? "";
    if (!id || !isTauri || !issue) {
      history = [];
      return;
    }
    const spanId = issue.span;
    api
      .historyList({ span: spanId })
      .then((h) => (history = h))
      .catch(() => (history = []));
    api
      .draftList(issue.page)
      .then((d) => {
        const mine = d.find((x) => x.span === spanId);
        if (mine && review.selected?.id === id) {
          editText = mine.text;
          editOpen = true;
          review.editing = { span: spanId, text: mine.text };
          ui.toast("Restored an unsaved edit for this word", "info", 4000);
        }
      })
      .catch(() => {});
  });

  // Evidence crop: the anchor box widened for context, from the backend
  // render at a readable scale.
  const CROP_H = 64;
  const crop = $derived.by(() => {
    if (!span || !span.anchors.length || !page) return null;
    const a = span.anchors[0]!.bbox;
    const pw = page.width_pt ?? 612;
    const padX = Math.max(90, a.w * 2.5);
    const x0 = Math.max(0, a.x - padX);
    const x1 = Math.min(pw, a.x + a.w + padX);
    const y0 = Math.max(0, a.y - a.h * 0.8);
    const y1 = a.y + a.h * 1.8;
    const zoom = CROP_H / (y1 - y0);
    const scale = pickScale(zoom, window.devicePixelRatio || 1);
    return { src: renderUrl(span.page, scale), scale, zoom, x0, y0, w: (x1 - x0) * zoom, h: CROP_H, word: { x: (a.x - x0) * zoom, y: (a.y - y0) * zoom, w: a.w * zoom, h: a.h * zoom } };
  });

  function chooseText(): string {
    if (!issue) return "";
    return issue.candidates[chosen]?.text ?? issue.original;
  }
  async function accept() {
    if (!issue) return;
    const text = chooseText();
    const original = issue.original;
    const kind = issue.kind;
    const out = await review.decide({ kind: "accept", text });
    if (out && text !== original && kind === "risky_substitution") ongroup(original, text);
  }
  function startEdit() {
    if (!issue) return;
    editOpen = true;
    review.editing = { span: issue.span, text: editText };
    setTimeout(() => editBox?.focus(), 0);
  }
  function onEditInput() {
    if (!issue) return;
    review.editing = { span: issue.span, text: editText };
    if (draftTimer) clearTimeout(draftTimer);
    const spanId = issue.span;
    const text = editText;
    draftTimer = setTimeout(() => {
      if (isTauri) void api.draftPut(spanId, text).catch(() => {});
    }, 400);
  }
  async function saveEdit() {
    if (!issue) return;
    const t = editText.trim();
    if (!t) {
      ui.toast("The reading cannot be empty", "warn");
      return;
    }
    if (draftTimer) clearTimeout(draftTimer);
    const out = await review.decide({ kind: "edit", text: t });
    if (out) editOpen = false;
  }
  async function cancelEdit() {
    if (draftTimer) clearTimeout(draftTimer);
    editOpen = false;
    const spanId = issue?.span;
    review.editing = null;
    if (spanId && isTauri) await api.draftDelete(spanId).catch(() => {});
    editText = issue?.original ?? "";
  }
  async function undo(h: HistoryEntry) {
    await review.undo(h.id);
  }
  function when(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleTimeString();
  }

  // Keyboard inside the tab: 1-9 pick candidates, A/E/S/L decide.
  export function onkey(e: KeyboardEvent): boolean {
    if (!issue || editOpen) return false;
    const k = e.key;
    if (/^[1-9]$/.test(k)) {
      const n = Number(k) - 1;
      if (n < issue.candidates.length) chosen = n;
      return true;
    }
    if (k === "a" || k === "A") void accept();
    else if (k === "e" || k === "E") startEdit();
    else if (k === "s" || k === "S") void review.decide({ kind: "skip" });
    else if (k === "l" || k === "L") void review.decide({ kind: "later" });
    else return false;
    return true;
  }
  const unresolvedOnPage = $derived(review.page === view.page ? review.pageUnresolved : 0);
</script>

{#if !issue}
  <div class="empty">
    {#if review.page === view.page && review.matching.length}
      <p>{review.matching.length} {review.deferredView ? "deferred" : "unresolved"} on this page.</p>
      <button type="button" class="ctl primary" onclick={() => review.select(review.matching[0]!.id)}>Open the first</button>
    {:else if review.counts.matching}
      <p>No issues match on this page. <b>{review.counts.matching}</b> in the book.</p>
      <button type="button" class="ctl primary" onclick={() => review.step(true)}>Next issue <kbd>J</kbd></button>
    {:else}
      <p class="muted">Nothing to review at this threshold{review.counts.above_threshold ? ` · ${review.counts.above_threshold} scored above ${review.threshold}%` : ""}{review.counts.deferred ? ` · ${review.counts.deferred} deferred` : ""}.</p>
    {/if}
    <p class="muted small">Unresolved {review.counts.unresolved} · matching {review.counts.matching} · deferred {review.counts.deferred} · resolved {review.counts.resolved}{review.counts.stale ? ` · stale ${review.counts.stale}` : ""}{review.counts.unprocessed_pages ? ` · ${review.counts.unprocessed_pages} pages not yet indexed` : ""}</p>
  </div>
{:else}
  <div class="head">
    <div class="strong">Issue {position >= 0 ? position + 1 : "–"} of {review.matching.length} on page {issue.page + 1}</div>
    <div class="muted small">{review.counts.matching} in book · {ISSUE_KIND_LABEL[issue.kind]}</div>
  </div>

  <div class="evidence" aria-label="Evidence">
    {#if crop}
      <div class="crop" style="height:{crop.h}px">
        <img src={crop.src} alt="Scan around the word" style="position:absolute;left:{-crop.x0 * crop.zoom}px;top:{-crop.y0 * crop.zoom}px;width:{(page?.width_pt ?? 612) * crop.zoom}px;height:{(page?.height_pt ?? 792) * crop.zoom}px" draggable="false" />
        <div class="box" style="left:{crop.word.x - 2}px;top:{crop.word.y - 2}px;width:{crop.word.w + 4}px;height:{crop.word.h + 4}px"></div>
      </div>
    {:else}
      <div class="crop none muted small">No scan anchor for this word</div>
    {/if}
    <div class="reading">
      <span class="serif">{span?.text ?? issue.original}</span>
      {#if issue.score !== null}
        <span class="chip" title="Score and its source">{issue.score}% · {issue.score_source}</span>
      {:else}
        <span class="chip">unscored · {issue.score_source}</span>
      {/if}
    </div>
    <p class="reason">{issue.reason}</p>
  </div>

  <div>
    <div class="label">Candidates</div>
    <ol class="cands" role="radiogroup" aria-label="Candidate readings">
      {#each issue.candidates as c, i (i)}
        <li>
          <button type="button" role="radio" aria-checked={chosen === i} class="cand" class:on={chosen === i} onclick={() => (chosen = i)}>
            <kbd>{c.proposal ? i + 1 : "raw"}</kbd>
            <span class="serif ctext">{c.text}</span>
            <span class="muted small">{c.score !== null ? `${c.score}` : ""} {c.source}</span>
          </button>
        </li>
      {/each}
    </ol>
  </div>

  {#if editOpen}
    <div class="edit">
      <label class="small" for="issue-edit">Edit the reading</label>
      <input id="issue-edit" type="text" bind:value={editText} bind:this={editBox} oninput={onEditInput} onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); void saveEdit(); } else if (e.key === "Escape") { e.preventDefault(); void cancelEdit(); } }} class="serif" />
      <div class="two">
        <button type="button" class="ctl" onclick={cancelEdit}>Discard</button>
        <button type="button" class="ctl primary" onclick={saveEdit}>Save edit</button>
      </div>
    </div>
  {:else}
    <div class="decisions">
      <button type="button" class="ctl primary" onclick={accept} disabled={ui.saveState === "saving"}>Accept <kbd>A</kbd></button>
      <button type="button" class="ctl" onclick={startEdit}>Edit <kbd>E</kbd></button>
      <button type="button" class="ctl" onclick={() => review.decide({ kind: "skip" })} disabled={ui.saveState === "saving"}>Skip <kbd>S</kbd></button>
      <button type="button" class="ctl" onclick={() => review.decide({ kind: "later" })} disabled={ui.saveState === "saving" || issue.status === "deferred"}>Later <kbd>L</kbd></button>
    </div>
  {/if}
  <label class="check"><input type="checkbox" bind:checked={review.autoAdvance} onchange={() => review.savePrefs()} /> Auto-advance to next issue</label>

  <div>
    <div class="label">History</div>
    {#if history.length === 0}
      <p class="muted small">OCR read “{issue.original}” · raw</p>
    {:else}
      <ul class="hist">
        {#each history as h (h.id)}
          <li class:undone={h.undone}>
            <span>{h.label}</span>
            <span class="muted small">{when(h.ts)}</span>
            {#if h.undoable && !h.undone}<button type="button" class="link" onclick={() => undo(h)}>Undo</button>{/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if span && !review.issues.some((i) => i.span === span.id && i.kind === "user_flag" && i.status === "open")}
      <button type="button" class="link" onclick={() => span && review.flag(span.id)}>Flag this word for later</button>
    {/if}
  </div>
  <p class="muted small">Page {issue.page + 1} · {unresolvedOnPage} unresolved</p>
{/if}

<style>
  .empty {
    display: grid;
    gap: 10px;
    font-size: 13px;
  }
  .empty p {
    margin: 0;
  }
  .head .strong {
    font-weight: 600;
    font-size: 13.5px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .evidence {
    display: grid;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
  }
  .crop {
    position: relative;
    overflow: hidden;
    border-radius: 4px;
    background: var(--scan-paper);
    border: 1px solid var(--border-input);
  }
  .crop.none {
    display: grid;
    place-items: center;
    height: 40px;
  }
  .crop img {
    max-width: none;
    user-select: none;
  }
  .box {
    position: absolute;
    border: 2px solid var(--accent);
    background: var(--accent-bg-scan);
    mix-blend-mode: multiply;
    border-radius: 2px;
  }
  .reading {
    display: flex;
    align-items: baseline;
    gap: 10px;
    justify-content: space-between;
  }
  .serif {
    font-family: var(--font-serif);
    font-size: 16px;
  }
  .chip {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 10px;
    background: var(--paper);
    border: 1px solid var(--border);
    white-space: nowrap;
  }
  .reason {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .label {
    margin-bottom: 4px;
  }
  .cands {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .cand {
    all: unset;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    box-sizing: border-box;
    padding: 5px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--paper);
    cursor: default;
  }
  .cand.on {
    border-color: var(--accent);
    background: var(--accent-bg);
  }
  .cand:focus-visible {
    outline: 2px solid var(--accent);
  }
  .ctext {
    overflow-wrap: anywhere;
  }
  kbd {
    font: 500 10.5px var(--font-mono);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--border-input);
    background: var(--raised);
    color: var(--muted);
  }
  .decisions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .ctl {
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
  .ctl.primary kbd {
    background: transparent;
    color: inherit;
    border-color: currentColor;
    opacity: 0.8;
  }
  .ctl:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .edit {
    display: grid;
    gap: 6px;
  }
  .edit input {
    padding: 6px 8px;
    border: 1px solid var(--border-input);
    border-radius: 6px;
    background: var(--paper);
    color: var(--text);
  }
  .edit input:focus-visible {
    outline: 2px solid var(--accent);
  }
  .two {
    display: flex;
    gap: 6px;
  }
  .two .ctl {
    flex: 1;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 12px;
  }
  .hist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .hist li {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 8px;
    align-items: baseline;
  }
  .hist li.undone {
    text-decoration: line-through;
    color: var(--muted);
  }
  .link {
    all: unset;
    color: var(--accent-text);
    cursor: pointer;
    font-size: 12px;
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--accent);
  }
</style>
