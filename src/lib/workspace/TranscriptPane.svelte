<script lang="ts">
  // Transcript pane (design 4.3, REV-02, REV-03). Header: title, issues on
  // this page, the review threshold slider. Body: running head in small
  // caps, logical paragraphs of the effective text, footnotes after a
  // divider. Marks: issue (amber), deferred (dotted), accepted (green),
  // auto-applied (light green); the focused issue carries a ring. Hover or
  // focus on a mark highlights the source word on the scan; click selects
  // the issue. Falls back to raw OCR lines before the text pass has run.
  import { api, errorMessage, type OcrWord, type PageRow, type Span } from "$lib/api";
  import { KIND_LABEL } from "$lib/stores/layout.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { TRANSCRIPT_ZOOMS, ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";
  import type { Snippet } from "svelte";

  type Props = { page: PageRow | null; header?: Snippet };
  let { page, header }: Props = $props();

  // ----- effective text -----
  type Word = { span: Span; mark: ReturnType<typeof review.markFor> };
  type Para = { kind: string; label: string; words: Word[]; region: string | null };
  const paras = $derived.by((): Para[] => {
    if (review.page !== view.page) return [];
    const byRegion = new Map(review.regions.map((r) => [r.id, r]));
    const out: Para[] = [];
    let cur: Para | null = null;
    let lastRegion: string | null | undefined;
    for (const sp of review.spans) {
      const r = sp.region ? byRegion.get(sp.region) : undefined;
      if (sp.paragraph_start || sp.region !== lastRegion || !cur) {
        cur = { kind: r?.kind ?? "uncertain", label: r ? KIND_LABEL[r.kind] : "outside regions", words: [], region: sp.region };
        out.push(cur);
        lastRegion = sp.region;
      }
      cur.words.push({ span: sp, mark: review.markFor(sp) });
    }
    return out;
  });
  const head = $derived(paras.filter((p) => p.kind === "header" || p.kind === "page_number"));
  const body = $derived(paras.filter((p) => !["header", "page_number", "footer", "footnote"].includes(p.kind)));
  const foot = $derived(paras.filter((p) => p.kind === "footer" || p.kind === "footnote"));
  const headLine = $derived(head.map((p) => p.words.map((w) => w.span.text).join(" ")).join(" · "));
  const onPage = $derived(review.page === view.page ? review.matching.length : 0);

  // ----- raw fallback -----
  type Line = { text: string; conf: number };
  let lines = $state<Line[]>([]);
  let rawFor = $state<number | null>(null);
  function toLines(ws: OcrWord[]): Line[] {
    const byLine = new Map<string, { words: string[]; conf: number[]; y: number }>();
    for (const w of ws) {
      const k = `${w.block}/${w.paragraph}/${w.line}`;
      const e = byLine.get(k) ?? { words: [], conf: [], y: w.bbox.y };
      if (w.text) e.words.push(w.text);
      e.conf.push(w.confidence);
      byLine.set(k, e);
    }
    return [...byLine.values()]
      .sort((a, b) => a.y - b.y)
      .filter((l) => l.words.length)
      .map((l) => ({ text: l.words.join(" "), conf: l.conf.reduce((a, b) => a + b, 0) / l.conf.length }));
  }
  $effect(() => {
    const idx = view.page;
    if (!page?.ocr_done || page.text_done) {
      lines = [];
      rawFor = idx;
      return;
    }
    let cancelled = false;
    api
      .pageOcr(idx)
      .then((o) => {
        if (cancelled) return;
        lines = o ? toLines(o.words) : [];
        rawFor = idx;
      })
      .catch(() => {
        if (!cancelled) lines = [];
      });
    return () => {
      cancelled = true;
    };
  });

  // Keep the focused issue in view.
  let bodyEl = $state<HTMLElement | null>(null);
  $effect(() => {
    const id = review.selectedId;
    if (!id || !bodyEl) return;
    const issue = review.selected;
    if (!issue) return;
    const el = bodyEl.querySelector<HTMLElement>(`[data-span="${issue.span}"]`);
    el?.scrollIntoView({ block: "center" });
  });

  function onMarkKey(e: KeyboardEvent, w: Word) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (w.mark.issue) review.select(w.mark.issue.id);
    }
  }
  // ----- free edit of selected text -----
  // Select any words in the transcript, then Edit text: the typed text
  // replaces them in one undoable step.
  type Picked = { from: number; to: number; x: number; y: number };
  let picked = $state<Picked | null>(null);
  let editor = $state<{ from: number; to: number; x: number; y: number; text: string } | null>(null);
  let editorBox = $state<HTMLTextAreaElement | null>(null);
  let saving = $state(false);
  let lastEdit = $state<{ history: string; words: number } | null>(null);

  function onSelect() {
    if (editor) return;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0 || !bodyEl || !bodyEl.contains(sel.anchorNode) || !bodyEl.contains(sel.focusNode)) {
      picked = null;
      return;
    }
    const range = sel.getRangeAt(0);
    // Every word the selection touches, ignoring one it only ends at.
    const touched = [...bodyEl.querySelectorAll<HTMLElement>("[data-span]")].filter(
      (el) => range.intersectsNode(el) && !(range.endOffset === 0 && el.contains(range.endContainer) && !el.contains(range.startContainer)),
    );
    const idx = touched.map((el) => review.spans.findIndex((s) => s.id === el.dataset.span)).filter((i) => i >= 0);
    if (idx.length === 0) {
      picked = null;
      return;
    }
    const a = Math.min(...idx);
    const b = Math.max(...idx);
    const rect = range.getBoundingClientRect();
    const host = bodyEl.getBoundingClientRect();
    picked = { from: Math.min(a, b), to: Math.max(a, b), x: rect.left - host.left + bodyEl.scrollLeft, y: rect.bottom - host.top + bodyEl.scrollTop + 4 };
  }
  function openEditor() {
    if (!picked) return;
    const words = review.spans.slice(picked.from, picked.to + 1);
    const text = words
      .map((s, i) => (s.text ? s.text + (i < words.length - 1 ? s.trailing : "") : ""))
      .join("")
      .replace(/\s+/g, " ")
      .trim();
    editor = { ...picked, text };
    picked = null;
    window.getSelection()?.removeAllRanges();
    setTimeout(() => {
      editorBox?.focus();
      editorBox?.select();
    }, 0);
  }
  function closeEditor() {
    editor = null;
  }
  async function saveEditor() {
    if (!editor || saving) return;
    const words = review.spans.slice(editor.from, editor.to + 1);
    saving = true;
    try {
      const out = await ui.save(() => api.textEdit(words.map((s) => ({ span: s.id, revision: s.revision })), editor!.text));
      editor = null;
      if (out.history) {
        lastEdit = { history: out.history, words: out.changed };
        ui.announce(`Edited ${out.changed} word${out.changed === 1 ? "" : "s"}`);
        await review.reload();
      }
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    } finally {
      saving = false;
    }
  }
  // A page change drops the pick and any open editor.
  $effect(() => {
    void view.page;
    picked = null;
    editor = null;
    lastEdit = null;
  });
  async function undoLast() {
    if (!lastEdit) return;
    const id = lastEdit.history;
    lastEdit = null;
    await review.undo(id);
  }
  const editWords = $derived(editor ? editor.to - editor.from + 1 : 0);

  function title(w: Word): string {
    const i = w.mark.issue;
    if (i) return `${i.reason}${i.score !== null ? ` · ${i.score}% · ${i.score_source}` : ""}`;
    if (w.mark.kind === "accepted") return "accepted reading";
    if (w.mark.kind === "auto") return "changed by the text pass";
    return "";
  }
</script>

<section class="transcript" aria-label="Transcript">
  <div class="header">
    <span class="strong">Transcript</span>
    {#if review.page === view.page && review.spans.length}
      <span class="muted count" aria-live="polite">{onPage} {review.deferredView ? "deferred" : onPage === 1 ? "issue" : "issues"}<span class="lbl"> on this page</span></span>
    {/if}
    <span class="grow"></span>
    <label class="slider">
      <span class="muted"><span class="lbl">Show issues </span>below</span>
      <input
        type="range"
        min="0"
        max="100"
        step="1"
        bind:value={review.threshold}
        onchange={() => {
          review.savePrefs();
          void review.refreshCounts();
        }}
        aria-label="Review threshold: show issues below this score"
        aria-valuetext="{review.threshold} percent"
      />
      <span class="mono">{review.threshold}%</span>
    </label>
    <span class="zoom" role="group" aria-label="Transcript zoom">
      <button type="button" class="zbtn" onclick={() => ui.zoomTranscript(-1)} disabled={ui.transcriptZoom <= TRANSCRIPT_ZOOMS[0]!} aria-label="Zoom transcript out">−</button>
      <button type="button" class="zpct" onclick={() => ui.zoomTranscript(0)} title="Reset to 100%" aria-live="polite">{ui.transcriptZoom}%</button>
      <button type="button" class="zbtn" onclick={() => ui.zoomTranscript(1)} disabled={ui.transcriptZoom >= TRANSCRIPT_ZOOMS[TRANSCRIPT_ZOOMS.length - 1]!} aria-label="Zoom transcript in">+</button>
    </span>
    {#if header}{@render header()}{/if}
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="scroller"
    bind:this={bodyEl}
    style="font-size:calc(var(--font-size-transcript) * {ui.transcriptZoom / 100})"
    onmouseup={onSelect}
    onkeyup={(e) => { if (e.shiftKey) onSelect(); }}
    onwheel={(e) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      ui.zoomTranscript(e.deltaY < 0 ? 1 : -1);
    }}
  >
    {#if lastEdit}
      <p class="undo-bar small" aria-live="polite">Edited {lastEdit.words} word{lastEdit.words === 1 ? "" : "s"}. <button type="button" class="link" onclick={undoLast}>Undo</button> <button type="button" class="link" onclick={() => (lastEdit = null)}>Dismiss</button></p>
    {/if}
    {#if picked}
      <button type="button" class="pick" style="left:{picked.x}px;top:{picked.y}px" onmousedown={(e) => e.preventDefault()} onclick={openEditor}>Edit text</button>
    {/if}
    {#if editor}
      <div class="editor" style="left:{editor.x}px;top:{editor.y}px" role="dialog" aria-label="Edit selected text">
        <textarea
          bind:this={editorBox}
          bind:value={editor.text}
          rows="3"
          aria-label="Text"
          onkeydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); void saveEditor(); }
            else if (e.key === "Escape") { e.preventDefault(); closeEditor(); }
          }}
        ></textarea>
        <div class="editor-row">
          <span class="muted small">{editWords} word{editWords === 1 ? "" : "s"} · Esc cancels</span>
          <span class="grow"></span>
          <button type="button" class="ctl" onclick={closeEditor}>Cancel</button>
          <button type="button" class="ctl primary" onclick={saveEditor} disabled={saving}>Save <kbd>Enter</kbd></button>
        </div>
      </div>
    {/if}
    {#if paras.length}
      {#if headLine}
        <p class="runhead" aria-label="Running head">{headLine}</p>
      {/if}
      {#each body as p, pi (pi)}
        <p class="para {p.kind}" data-label={p.label}>
          {#each p.words as w (w.span.id)}{#if w.mark.issue && w.mark.kind !== "none"}<button
              type="button"
              class="mark {w.mark.kind}"
              class:focused={review.selected?.span === w.span.id}
              class:hover={review.hoverSpan === w.span.id}
              data-span={w.span.id}
              aria-label="{w.span.text}: {w.mark.issue.kind.replace('_', ' ')}"
              title={title(w)}
              onmouseenter={() => (review.hoverSpan = w.span.id)}
              onmouseleave={() => (review.hoverSpan = null)}
              onfocus={() => (review.hoverSpan = w.span.id)}
              onblur={() => (review.hoverSpan = null)}
              onclick={() => review.select(w.mark.issue!.id)}
              onkeydown={(e) => onMarkKey(e, w)}>{w.span.text}</button>{:else if w.mark.kind !== "none"}<mark
              class="mark {w.mark.kind}"
              class:focused={review.selected?.span === w.span.id}
              data-span={w.span.id}
              title={title(w)}>{w.span.text}</mark>{:else}<span
              data-span={w.span.id}
              class:focused={review.selected?.span === w.span.id}>{w.span.text}</span>{/if}{w.span.trailing}{/each}
        </p>
      {/each}
      {#if foot.length}
        <hr class="divider" />
        {#each foot as p, pi (pi)}
          <p class="para {p.kind}" data-label={p.label}>
            {#each p.words as w (w.span.id)}{#if w.mark.issue && w.mark.kind !== "none"}<button type="button" class="mark {w.mark.kind}" class:focused={review.selected?.span === w.span.id} data-span={w.span.id} aria-label="{w.span.text}: {w.mark.issue.kind.replace('_', ' ')}" title={title(w)} onmouseenter={() => (review.hoverSpan = w.span.id)} onmouseleave={() => (review.hoverSpan = null)} onfocus={() => (review.hoverSpan = w.span.id)} onblur={() => (review.hoverSpan = null)} onclick={() => review.select(w.mark.issue!.id)} onkeydown={(e) => onMarkKey(e, w)}>{w.span.text}</button>{:else if w.mark.kind !== "none"}<mark class="mark {w.mark.kind}" data-span={w.span.id} title={title(w)}>{w.span.text}</mark>{:else}<span data-span={w.span.id}>{w.span.text}</span>{/if}{w.span.trailing}{/each}
          </p>
        {/each}
      {/if}
    {:else if review.loading && review.page !== view.page}
      <p class="muted note">Loading…</p>
    {:else if lines.length && rawFor === view.page}
      <p class="muted note">Raw OCR lines · the text pass has not run on this page yet.</p>
      {#each lines as l, i (i)}
        <p class="line" class:low={l.conf < 70}>{l.text}</p>
      {/each}
    {:else}
      <p class="muted note">
        No text for page {view.page + 1} yet.
        {#if page?.status === "queued"}It is queued for processing.{:else if page?.status === "unprocessed"}It is outside the processing scope.{:else if page?.status === "failed"}Processing failed: {page.error}{/if}
      </p>
    {/if}
  </div>
</section>

<style>
  .transcript {
    background: var(--paper);
    display: grid;
    grid-template-rows: var(--pane-header-h) 1fr;
    grid-template-columns: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
  }
  .header {
    display: flex;
    min-width: 0;
    overflow: hidden;
    container-type: inline-size;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-soft);
    white-space: nowrap;
  }
  .strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }
  .grow {
    flex: 1;
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .slider input {
    width: 80px;
    min-width: 50px;
    accent-color: var(--accent);
  }
  @container (max-width: 560px) {
    .lbl {
      display: none;
    }
  }
  /* Keep the zoom buttons reachable in a narrow pane. */
  @container (max-width: 420px) {
    .count,
    .slider > .muted,
    .slider .mono {
      display: none;
    }
    .slider input {
      width: 56px;
    }
  }
  @container (max-width: 360px) {
    .zoom .zpct {
      display: none;
    }
  }
  .zoom {
    flex-shrink: 0;
  }
  .zoom {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .zbtn,
  .zpct {
    all: unset;
    padding: 3px 8px;
    border-radius: 5px;
    border: 1px solid var(--border-input);
    background: var(--paper);
    cursor: default;
    line-height: 1.2;
  }
  .zpct {
    min-width: 36px;
    text-align: center;
    border-color: transparent;
    background: none;
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .zbtn:disabled {
    opacity: 0.4;
  }
  .zbtn:focus-visible,
  .zpct:focus-visible {
    outline: 2px solid var(--accent);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
    min-width: 32px;
  }
  .scroller {
    position: relative;
    overflow: auto;
    padding: 18px 26px;
    font-family: var(--font-serif);
    font-size: var(--font-size-transcript);
    line-height: var(--line-height-transcript);
    color: var(--text);
    user-select: text;
  }
  .pick {
    position: absolute;
    z-index: 3;
    padding: 3px 9px;
    border-radius: 6px;
    border: 1px solid var(--primary-bg);
    background: var(--primary-bg);
    color: var(--primary-fg);
    font: 600 12px var(--font-ui);
    cursor: pointer;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.18);
  }
  .editor {
    position: absolute;
    z-index: 3;
    width: min(420px, calc(100% - 32px));
    display: grid;
    gap: 6px;
    padding: 8px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--raised);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.2);
    font-family: var(--font-ui);
  }
  .editor textarea {
    resize: vertical;
    padding: 6px 8px;
    border: 1px solid var(--border-input);
    border-radius: 6px;
    background: var(--paper);
    color: var(--text);
    font: inherit;
    font-family: var(--font-serif);
    font-size: 15px;
  }
  .editor textarea:focus-visible {
    outline: 2px solid var(--accent);
  }
  .editor-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ctl {
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    font: 500 12px var(--font-ui);
    cursor: pointer;
    display: inline-flex;
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
    background: transparent;
    color: inherit;
    font: 500 10px var(--font-mono);
    padding: 0 4px;
    border-radius: 3px;
    border: 1px solid currentColor;
    opacity: 0.8;
  }
  .small {
    font-size: 12px;
  }
  .undo-bar {
    position: sticky;
    top: -18px;
    z-index: 2;
    margin: -18px -26px 12px;
    padding: 6px 26px;
    background: var(--raised);
    border-bottom: 1px solid var(--border-soft);
    font-family: var(--font-ui);
  }
  .link {
    all: unset;
    color: var(--accent-text);
    cursor: pointer;
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--accent);
  }
  .runhead {
    margin: 0 0 14px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-soft);
    font-variant: small-caps;
    letter-spacing: 0.08em;
    font-size: 0.85em;
    color: var(--text-2);
  }
  .para {
    margin: 0 0 0.9em;
    text-wrap: pretty;
    white-space: pre-wrap;
  }
  .para.heading {
    font-weight: 600;
    text-align: center;
  }
  .para.marginalia,
  .para.footnote,
  .para.footer {
    font-size: 0.86em;
    color: var(--text-2);
  }
  .para.marginalia::before,
  .para.footnote::before,
  .para.footer::before,
  .para.uncertain::before {
    content: attr(data-label);
    display: block;
    font: 500 10px var(--font-ui);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .divider {
    border: 0;
    border-top: 1px solid var(--border-soft);
    margin: 12px 0 14px;
  }
  .mark {
    background: transparent;
    color: inherit;
    font: inherit;
    padding: 0;
    border: 0;
    border-radius: 2px;
    text-decoration-thickness: 2px;
    text-underline-offset: 3px;
    cursor: pointer;
    user-select: text;
  }
  .mark.accepted {
    background: var(--ok-bg);
    text-decoration: underline var(--ok);
  }
  .mark.auto {
    text-decoration: underline var(--ok);
    text-decoration-style: dotted;
  }
  .mark.issue {
    background: var(--accent-bg);
    text-decoration: underline var(--accent);
  }
  .mark.deferred {
    text-decoration: underline dotted var(--accent);
  }
  .mark.focused,
  span.focused {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-radius: 2px;
  }
  .mark.hover {
    outline: 1px solid var(--text);
    outline-offset: 1px;
  }
  .mark:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .line {
    margin: 0;
    text-wrap: pretty;
  }
  .line.low {
    color: var(--accent-text);
  }
  .note {
    font-family: var(--font-ui);
    font-size: 13px;
  }
</style>
