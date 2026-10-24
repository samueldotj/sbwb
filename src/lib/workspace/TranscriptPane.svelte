<script lang="ts">
  // Transcript pane (design 4.3, REV-02, REV-03). Header: title, issues on
  // this page, the review threshold slider. Body: running head in small
  // caps, logical paragraphs of the effective text, footnotes after a
  // divider. Marks: issue (amber), deferred (dotted), accepted (green),
  // auto-applied (light green); the focused issue carries a ring. Hover or
  // focus on a mark highlights the source word on the scan; click selects
  // the issue. Falls back to raw OCR lines before the text pass has run.
  import { api, errorMessage, type HistoryEntry, type OcrWord, type PageRow, type Span } from "$lib/api";
  import { checkGrammar, type GrammarFix, type GrammarFlag } from "$lib/grammar";
  import { charRuns, countKinds } from "$lib/highlight";
  import { romanInfo, type RomanInfo } from "$lib/roman";
  import { isTauri } from "$lib/ipc";
  import { KIND_LABEL } from "$lib/stores/layout.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { TRANSCRIPT_ZOOMS, ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";
  import { untrack, type Snippet } from "svelte";

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
  // Text outside every region, hidden on request; a paragraph holding the
  // selected issue stays so its word can still be decided.
  const outsideWords = $derived(body.filter((p) => p.region === null).reduce((n, p) => n + p.words.length, 0));
  const shownBody = $derived(review.hideOutside ? body.filter((p) => p.region !== null || p.words.some((w) => w.span.id === review.selected?.span)) : body);
  const foot = $derived(paras.filter((p) => p.kind === "footer" || p.kind === "footnote"));
  const headLine = $derived(head.map((p) => p.words.map((w) => w.span.text).join(" ")).join(" · "));
  // Highlight numbers / special characters (a menu in the header); the
  // counts cover the text drawn on the page.
  let hlOpen = $state(false);
  let hlRoot = $state<HTMLElement | null>(null);
  const hlOn = $derived(review.highlightNumbers || review.highlightPunctuation || review.highlightSpecial);
  const kindCounts = $derived(countKinds([headLine, ...[...shownBody, ...foot].flatMap((p) => p.words.map((w) => w.span.text))]));
  function onWindowClick(e: MouseEvent) {
    if (hlOpen && hlRoot && !hlRoot.contains(e.target as Node)) hlOpen = false;
    if (wordMenu && !(e.target as Element | null)?.closest?.(".word-pop")) wordMenu = null;
  }

  // ----- spelling: words missing from the chosen dictionary get a wavy
  // underline; right-click one for suggestions or to add it to the book -----
  const DIALECT_LABEL = { en_GB: "UK English", en_US: "US English" } as const;
  let misspelt = $state<Set<string>>(new Set());
  let vocabTick = $state(0);
  let spellToken = 0;
  const pageTexts = $derived(review.page === view.page ? [...new Set(review.spans.map((s) => s.text).filter(Boolean))] : []);
  $effect(() => {
    const dialect = review.spelling;
    const words = pageTexts;
    void vocabTick;
    const t = ++spellToken;
    if (dialect === "off" || !words.length || !isTauri) {
      misspelt = new Set();
      return;
    }
    api
      .spellCheck(words, dialect)
      .then((r) => {
        if (t === spellToken) misspelt = new Set(r);
      })
      .catch((e) => {
        if (t !== spellToken) return;
        misspelt = new Set();
        ui.toast(`Spell check unavailable: ${errorMessage(e)}`, "error", 6000);
      });
  });
  // Grammar: simple spacing, punctuation and capital checks, worked out here.
  const grammarFlags = $derived<Map<string, GrammarFlag[]>>(review.grammar && review.page === view.page ? checkGrammar(review.spans) : new Map());
  const grammarCount = $derived([...shownBody, ...foot].reduce((n, p) => n + p.words.reduce((k, w) => k + (grammarFlags.get(w.span.id)?.length ?? 0), 0), 0));
  const misspeltCount = $derived([...shownBody, ...foot].reduce((n, p) => n + p.words.filter((w) => misspelt.has(w.span.text)).length, 0));
  /** The word without surrounding punctuation or quotes. */
  function coreOf(t: string): string {
    return t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
  }
  // ----- word menu: a single click (or a right-click on a misspelt word)
  // opens it under the word: link the word to one of the page's footnotes,
  // spelling suggestions when it is underlined, and Edit text -----
  type WordMenu = { span: string; core: string; x: number; y: number; above: boolean; inNote: boolean; dialect: "en_GB" | "en_US" | null; suggestions: string[] | null; flags: GrammarFlag[]; roman: RomanInfo | null; para: "split" | "join" | null };
  let wordMenu = $state<WordMenu | null>(null);
  let clickTimer: ReturnType<typeof setTimeout> | null = null;
  function openWordMenu(el: HTMLElement) {
    const span = el.dataset.span ? review.spanById(el.dataset.span) : null;
    if (!span || !bodyEl) return;
    const host = bodyEl.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    const core = coreOf(span.text);
    const dialect = review.spelling !== "off" && misspelt.has(span.text) ? review.spelling : null;
    revisit = null;
    picked = null;
    review.pickedSpan = span.id;
    const inNote = foot.some((p) => p.kind === "footnote" && p.words.some((w) => w.span.id === span.id));
    // Below the word, or above it when the pane has little room left below.
    const above = host.bottom - r.bottom < 320 && r.top - host.top > host.bottom - r.bottom;
    const y = above ? r.top - host.top + bodyEl.scrollTop - 4 : r.bottom - host.top + bodyEl.scrollTop + 4;
    wordMenu = { span: span.id, core, x: popLeft(r.left - host.left), y, above, inNote, dialect, suggestions: null, flags: grammarFlags.get(span.id) ?? [], roman: romanInfo(span.text), para: paragraphAction(span.id) };
    if (!dialect) return;
    const done = (s: string[]) => {
      if (wordMenu?.span === span.id) wordMenu = { ...wordMenu, suggestions: s };
    };
    api.spellSuggest(core, dialect).then(done, () => done([]));
  }
  /** What the word menu can do with paragraphs at this word: start one
   *  here, or (for a paragraph's first word, or a word after a line break
   *  an earlier join left) join it to the paragraph before when that is
   *  in the same region. */
  function paragraphAction(id: string): "split" | "join" | null {
    const i = review.spans.findIndex((s) => s.id === id);
    const s = review.spans[i];
    if (!s) return null;
    let k = i - 1;
    while (k >= 0 && !review.spans[k]!.text) k--;
    const prev = review.spans[k];
    if (!prev || prev.region !== s.region) return null;
    return s.paragraph_start || prev.trailing.includes("\n") ? "join" : "split";
  }
  async function setParagraph(start: boolean) {
    const m = wordMenu;
    if (!m) return;
    wordMenu = null;
    try {
      await ui.save(() => api.paragraphSet(m.span, start));
      await review.reload();
      ui.announce(start ? "New paragraph" : "Paragraphs joined");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  /** A plain click on a word; the delay lets a double-click edit instead. */
  function onWordClick(e: MouseEvent) {
    if (editor) return;
    const t = e.target as Element | null;
    if (!t || t.closest("button, .cands, .editor, .pick, sup, input, textarea")) return;
    const el = t.closest<HTMLElement>("[data-span]");
    const sel = window.getSelection();
    if (!el || (sel && !sel.isCollapsed)) return;
    if (clickTimer) clearTimeout(clickTimer);
    clickTimer = setTimeout(() => {
      clickTimer = null;
      openWordMenu(el);
    }, 250);
  }
  function onContextMenu(e: MouseEvent) {
    const el = (e.target as Element | null)?.closest<HTMLElement>("[data-span]");
    const span = el?.dataset.span ? review.spanById(el.dataset.span) : null;
    if (!el || !span || !(misspelt.has(span.text) || grammarFlags.has(span.id) || romanInfo(span.text))) return;
    e.preventDefault();
    openWordMenu(el);
  }
  /** Link the word to a footnote (moving the note's link if it had one). */
  async function linkTo(note: string) {
    const m = wordMenu;
    if (!m) return;
    wordMenu = null;
    await review.footnote(() => api.footnoteLink(m.span, note));
    ui.announce("Footnote linked");
  }
  /** The note's opening, for the Link to list. */
  function noteHead(preview: string): string {
    const t = preview.trim();
    return t.length > 10 ? `${t.slice(0, 10)}…` : t;
  }
  function editFromMenu() {
    const m = wordMenu;
    const i = m ? review.spans.findIndex((x) => x.id === m.span) : -1;
    if (!m || i < 0) return;
    wordMenu = null;
    picked = { from: i, to: i, x: m.x, y: m.y - 4 };
    openEditor();
  }
  /** Apply a grammar fix: one undoable text edit of the words it names. */
  async function applyFix(f: GrammarFix) {
    const items = f.spans.map((id) => review.spanById(id)).filter((s) => s != null).map((s) => ({ span: s.id, revision: s.revision }));
    wordMenu = null;
    if (items.length !== f.spans.length) return;
    try {
      await ui.save(() => api.textEdit(items, f.text));
      await review.reload();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  /** Replace part of the word (`from`, the word without its punctuation),
   *  keeping the rest: an undoable text edit. */
  async function replaceInWord(from: string, to: string) {
    const p = wordMenu;
    const span = p ? review.spanById(p.span) : null;
    if (!p || !span) return;
    wordMenu = null;
    try {
      await ui.save(() => api.textEdit([{ span: span.id, revision: span.revision }], span.text.replace(from, to)));
      await review.reload();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  /** Replace the word with a spelling suggestion, keeping its punctuation. */
  function useSuggestion(s: string) {
    if (wordMenu) void replaceInWord(wordMenu.core, s);
  }
  async function addToBook() {
    const p = wordMenu;
    if (!p) return;
    wordMenu = null;
    try {
      await ui.save(() => api.vocabAdd(p.core, "vocab"));
      vocabTick++;
      ui.toast(`Added “${p.core}” to this book's words`, "ok", 3000);
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  const onPage = $derived(review.page === view.page ? review.matching.length : 0);
  const corrections = $derived(review.page === view.page ? review.spans.filter((s) => s.origin !== "ocr" && s.text).length : 0);

  // ----- candidates at the word (the selected issue) -----
  // The selected issue's candidates (one or more) in an editable dropdown under its word:
  // pick one to accept it, or type a reading and press Enter.
  let candText = $state("");
  let candPos = $state<{ x: number; y: number } | null>(null);
  let candDismissed = $state<string | null>(null);
  let candBusy = $state(false);
  const candIssue = $derived.by(() => {
    const i = review.selected;
    if (!i || i.status !== "open" || review.page !== view.page || candDismissed === i.id) return null;
    // every word issue, even with one candidate (or none: type a reading);
    // structural issues have a box instead of a word
    return i.bbox !== null && i.candidates.length === 0 ? null : i;
  });
  // Reset only when a different issue is selected: a refresh of the page's
  // issues must not throw away what the reader typed.
  const candId = $derived(candIssue?.id ?? null);
  $effect(() => {
    const id = candId;
    const i = untrack(() => candIssue);
    candText = i ? (i.candidates.find((c) => c.proposal)?.text ?? i.candidates[0]?.text ?? i.original) : "";
    void id;
  });
  $effect(() => {
    const id = candId;
    void ui.transcriptZoom; // re-place after a zoom
    const i = untrack(() => candIssue);
    candPos = null;
    if (!id || !i || !bodyEl) return;
    // place it under the word once the word is on screen
    requestAnimationFrame(() => {
      const el = bodyEl?.querySelector<HTMLElement>(`[data-span="${i.span}"]`);
      if (!el || !bodyEl) return;
      const host = bodyEl.getBoundingClientRect();
      const r = el.getBoundingClientRect();
      candPos = { x: popLeft(r.left - host.left), y: r.bottom - host.top + bodyEl.scrollTop + 4 };
    });
  });
  /** Left edge for a popover under a word, kept inside the transcript. */
  function popLeft(wordLeft: number): number {
    if (!bodyEl) return wordLeft;
    const width = Math.min(340, bodyEl.clientWidth - 32) + 16;
    return Math.max(8, Math.min(wordLeft, bodyEl.clientWidth - width - 8)) + bodyEl.scrollLeft;
  }
  async function chooseCandidate(text: string) {
    const i = candIssue;
    const t = text.trim();
    if (!i || !t || candBusy) return;
    candBusy = true;
    const isCandidate = i.candidates.some((c) => c.text === t);
    await review.decide(isCandidate ? { kind: "accept", text: t } : { kind: "edit", text: t });
    candBusy = false;
  }

  // ----- track corrections: revisit a corrected word -----
  type Revisit = { span: string; x: number; y: number; history: HistoryEntry[]; was: string | null; choices: string[] };
  let revisit = $state<Revisit | null>(null);
  async function openRevisit(spanId: string, el: HTMLElement) {
    if (!bodyEl) return;
    const host = bodyEl.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    const auto = review.proposals.find((p) => p.span === spanId && p.status === "applied_auto");
    const decided = review.issues.find((i) => i.span === spanId && i.status === "resolved");
    const was = decided?.original ?? auto?.original ?? null;
    const now = review.spanById(spanId)?.text;
    const choices = [...new Set((decided?.candidates ?? []).map((c) => c.text))].filter((c) => c !== now);
    revisit = { span: spanId, x: popLeft(r.left - host.left), y: r.bottom - host.top + bodyEl.scrollTop + 4, history: [], was, choices };
    try {
      const history = await api.historyList({ span: spanId });
      if (revisit?.span === spanId) revisit = { ...revisit, history };
    } catch {
      /* no history */
    }
  }
  async function undoEntry(id: string) {
    revisit = null;
    await review.undo(id);
  }
  /** Put a different reading on a corrected word (an undoable text edit). */
  async function setReading(text: string) {
    const r = revisit;
    const span = r ? review.spanById(r.span) : null;
    if (!r || !span) return;
    revisit = null;
    try {
      await ui.save(() => api.textEdit([{ span: span.id, revision: span.revision }], text));
      await review.reload();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  function onTrackClick(e: MouseEvent) {
    const el = review.trackChanges ? (e.target as Element | null)?.closest<HTMLElement>("mark.accepted[data-span], mark.auto[data-span]") : null;
    if (el?.dataset.span) void openRevisit(el.dataset.span, el);
    else onWordClick(e);
  }

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
    review.pickedSpan = review.spans[b]?.id ?? null;
    picked = { from: Math.min(a, b), to: Math.max(a, b), x: rect.left - host.left + bodyEl.scrollLeft, y: rect.bottom - host.top + bodyEl.scrollTop + 4 };
  }
  /** Double-click a word: edit that one word straight away. */
  function onDoubleClick(e: MouseEvent) {
    if (clickTimer) clearTimeout(clickTimer);
    clickTimer = null;
    wordMenu = null;
    if (editor || !bodyEl) return;
    const el = (e.target as Element | null)?.closest?.("[data-span]") as HTMLElement | null;
    if (!el) return;
    const i = review.spans.findIndex((s) => s.id === el.dataset.span);
    if (i < 0) return;
    e.preventDefault();
    const rect = el.getBoundingClientRect();
    const host = bodyEl.getBoundingClientRect();
    review.pickedSpan = review.spans[i]?.id ?? null;
    picked = { from: i, to: i, x: rect.left - host.left + bodyEl.scrollLeft, y: rect.bottom - host.top + bodyEl.scrollTop + 4 };
    openEditor();
  }
  function openEditor() {
    if (!picked) return;
    const words = review.spans.slice(picked.from, picked.to + 1);
    const text = words
      .map((s, i) => (s.text ? s.text + (i < words.length - 1 ? s.trailing : "") : ""))
      .join("")
      .replace(/\s+/g, " ")
      .trim();
    // Keep the box inside the pane when the words sit near its right edge.
    const paneW = bodyEl?.clientWidth ?? 0;
    const boxW = Math.min(420, paneW - 32);
    const left = bodyEl?.scrollLeft ?? 0;
    const x = paneW ? Math.max(left, Math.min(picked.x, left + paneW - boxW - 16)) : picked.x;
    editor = { ...picked, x, text };
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
    wordMenu = null;
    editor = null;
    lastEdit = null;
  });
  // Undone some other way (Ctrl+Z, history): the bar has nothing left to undo.
  $effect(() => {
    if (lastEdit && review.lastUndone === lastEdit.history) lastEdit = null;
  });
  async function undoLast() {
    if (!lastEdit) return;
    const id = lastEdit.history;
    lastEdit = null;
    await review.undo(id);
  }
  const editWords = $derived(editor ? editor.to - editor.from + 1 : 0);

  // ----- footnote references (TXT-02) -----
  const fnByAnchor = $derived(new Map(review.footnotes.refs.map((r) => [r.anchor, r])));
  const fnByNote = $derived(new Map(review.footnotes.refs.filter((r) => r.status === "linked").map((r) => [r.note, r])));

  /** Any word under the pointer (or focused), marked or not, points at its box on the scan. */
  function pointAtWord(e: Event) {
    const id = (e.target as Element | null)?.closest?.("[data-span]")?.getAttribute("data-span") ?? null;
    if (id !== review.hoverSpan) review.hoverSpan = id;
  }

  function title(w: Word): string {
    const i = w.mark.issue;
    if (i) return `${i.reason}${i.score !== null ? ` · ${i.score}% · ${i.score_source}` : ""}`;
    if (w.mark.kind === "accepted") return "accepted reading";
    if (w.mark.kind === "auto") return "changed by the text pass";
    return "";
  }
</script>

{#snippet fnref(id: string)}
  {@const r = fnByAnchor.get(id)}
  {#if r}<sup class="fnref" class:sug={r.status === "suggested"} title={r.status === "suggested" ? "Suggested footnote reference: accept or reject it in the Text pass tab" : `Footnote ${r.number}`}>{r.number ?? "?"}</sup>{/if}
{/snippet}
{#snippet wtext(t: string, id: string | null = null)}{@const gf = id ? grammarFlags.get(id) : undefined}{#if gf}<span class="gram" title={gf.map((f) => f.message).join(" · ")}>{@render spell(t)}</span>{:else}{@render spell(t)}{/if}{/snippet}
{#snippet spell(t: string)}{#if misspelt.has(t)}<span class="misspelt" title="Not in the {review.spelling === 'en_US' ? 'US' : 'UK'} English dictionary · right-click for suggestions">{@render chars(t)}</span>{:else}{@render chars(t)}{/if}{/snippet}
{#snippet chars(t: string)}{#if hlOn}{#each charRuns(t, { numbers: review.highlightNumbers, punctuation: review.highlightPunctuation, special: review.highlightSpecial }) as r, ri (ri)}{#if r.kind}<span class="hl {r.kind}">{r.text}</span>{:else}{r.text}{/if}{/each}{:else}{t}{/if}{/snippet}

<svelte:window onclick={onWindowClick} />

{#snippet fnnum(id: string)}
  {@const r = fnByNote.get(id)}
  {#if r}<sup class="fnnum" title="Footnote {r.number}">{r.number}</sup>{/if}
{/snippet}

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
    <button type="button" class="track-btn" class:on={review.trackChanges} aria-pressed={review.trackChanges} onclick={() => { review.trackChanges = !review.trackChanges; revisit = null; review.savePrefs(); }} title="Track corrections: box every corrected word in green; click one to see what it was and change or undo it">
      Corrections&nbsp;<span class="mono-n">{corrections}</span>
    </button>
    <div class="hl-menu" bind:this={hlRoot}>
      <button type="button" class="track-btn" class:on={hlOn || review.spelling !== "off" || review.grammar} aria-haspopup="true" aria-expanded={hlOpen} onclick={() => (hlOpen = !hlOpen)} title="Highlight numbers, punctuation, special characters, spelling mistakes or grammar problems in the transcript">Highlight ▾</button>
      {#if hlOpen}
        <div class="hl-pop" role="group" aria-label="Highlight characters">
          <label><input type="checkbox" bind:checked={review.highlightNumbers} onchange={() => review.savePrefs()} /> <span class="hl num">12</span> Numbers <span class="mono-n">{kindCounts.num}</span></label>
          <label><input type="checkbox" bind:checked={review.highlightPunctuation} onchange={() => review.savePrefs()} /> <span class="hl punct">.,;</span> Punctuation <span class="mono-n">{kindCounts.punct}</span></label>
          <label><input type="checkbox" bind:checked={review.highlightSpecial} onchange={() => review.savePrefs()} /> <span class="hl sym">¢*</span> Special characters <span class="mono-n">{kindCounts.sym}</span></label>
          <div class="hl-sep" role="separator"></div>
          <label><span class="misspelt">abc</span> Spelling
            <select bind:value={review.spelling} onchange={() => review.savePrefs()} aria-label="Spelling dictionary">
              <option value="off">Off</option>
              <option value="en_GB">UK English</option>
              <option value="en_US">US English</option>
            </select>
            {#if review.spelling !== "off"}<span class="mono-n">{misspeltCount}</span>{/if}
          </label>
          <label><input type="checkbox" bind:checked={review.grammar} onchange={() => review.savePrefs()} /> <span class="gram">a,b</span> Grammar {#if review.grammar}<span class="mono-n">{grammarCount}</span>{/if}</label>
          {#if review.spelling !== "off" || review.grammar}<span class="muted small">Click an underlined word for suggestions{review.spelling !== "off" ? " or to add it to this book" : ""}.</span>{/if}
        </div>
      {/if}
    </div>
    {#if outsideWords > 0}
      <button type="button" class="track-btn" onclick={() => { review.hideOutside = !review.hideOutside; review.savePrefs(); }} title="{review.hideOutside ? 'Show' : 'Hide'} the text that lies outside every layout region in the transcript; only what is shown changes">
        {review.hideOutside ? "Show" : "Hide"} outside&nbsp;<span class="mono-n">{outsideWords}</span>
      </button>
    {/if}
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
    class:track={review.trackChanges}
    bind:this={bodyEl}
    onclick={onTrackClick}
    style="font-size:calc(var(--font-size-transcript) * {ui.transcriptZoom / 100})"
    onmouseup={onSelect}
    ondblclick={onDoubleClick}
    oncontextmenu={onContextMenu}
    onmouseover={pointAtWord}
    onfocusin={pointAtWord}
    onmouseleave={() => (review.hoverSpan = null)}
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
    {#if candIssue && candPos}
      <div class="cands" style="left:{candPos.x}px;top:{candPos.y}px" role="dialog" aria-label="Candidates for {candIssue.original}">
        <div class="cands-row">
          <input
            type="text"
            bind:value={candText}
            aria-label="Reading"
            onkeydown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); void chooseCandidate(candText); }
              else if (e.key === "Escape") { e.preventDefault(); candDismissed = candIssue?.id ?? null; }
            }}
          />
          <button type="button" class="ctl primary" onclick={() => chooseCandidate(candText)} disabled={candBusy}>Use <kbd>Enter</kbd></button>
          <button type="button" class="ctl x" onclick={() => (candDismissed = candIssue?.id ?? null)} aria-label="Close">✕</button>
        </div>
        <ul class="cands-list">
          {#each candIssue.candidates as c, i (i)}
            <li>
              <button type="button" class:on={c.text === candText} onclick={() => chooseCandidate(c.text)} disabled={candBusy}>
                <span class="ctext">{c.text}</span>
                <span class="muted small">{c.score !== null ? `${c.score}% · ` : ""}{c.source}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
    {#if wordMenu}
      {@const m = wordMenu}
      {@const notes = m.inNote ? [] : review.footnotes.notes}
      <div class="cands word-pop" class:above={m.above} style="left:{m.x}px;top:{m.y}px" role="dialog" aria-label="Word {m.core}">
        <div class="cands-row">
          <b class="serif">{m.core || review.spanById(m.span)?.text}</b>
          <span class="grow"></span>
          <button type="button" class="ctl x" onclick={() => (wordMenu = null)} aria-label="Close">✕</button>
        </div>
        {#if notes.length}
          <div class="menu-label">Link to</div>
          <ul class="notes">
            {#each notes as n (n.span)}
              {@const ref = fnByNote.get(n.span)}
              {@const here = ref?.anchor === m.span}
              <li>
                <button type="button" class="note-btn" disabled={here} onclick={() => linkTo(n.span)} title={here ? "Already linked to this word" : ref ? `Linked to another word; this moves it here` : `Link footnote ${n.marker || ""} to this word`}>
                  <span class="mono-n">{ref?.number ?? (n.marker || "·")}</span>
                  <span class="serif">{noteHead(n.preview)}</span>
                  {#if here}<span class="muted small">linked</span>{:else if ref}<span class="muted small">move</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if m.roman}
          {@const ro = m.roman}
          <div class="menu-label">Roman numeral</div>
          {#if ro.value !== null}
            <span class="small"><span class="serif">{ro.core}</span> = {ro.value}{#if ro.standard} · usually written <span class="serif">{ro.standard}</span>{/if}</span>
            <div class="cands-row wrap">
              <button type="button" class="ctl" onclick={() => replaceInWord(ro.core, String(ro.value))}>Convert → {ro.value}</button>
              {#if ro.standard}<button type="button" class="ctl" onclick={() => replaceInWord(ro.core, ro.standard!)}>Use <span class="serif">{ro.standard}</span></button>{/if}
            </div>
          {:else}
            <span class="small">“<span class="serif">{ro.core}</span>” is not a well-formed numeral; it may be a misreading of:</span>
            {#each ro.readings as rd (rd.text)}
              <div class="cands-row wrap">
                <button type="button" class="ctl" onclick={() => replaceInWord(ro.core, rd.text)} title="Correct the numeral">Use <span class="serif">{rd.text}</span> ({rd.value})</button>
                <button type="button" class="ctl" onclick={() => replaceInWord(ro.core, String(rd.value))} title="Correct it and write it as a number">Convert → {rd.value}</button>
              </div>
            {/each}
          {/if}
        {/if}
        {#if m.dialect}
          <div class="menu-label">Not in the {DIALECT_LABEL[m.dialect]} dictionary</div>
          <div class="cands-row wrap">
            {#if m.suggestions === null}
              <span class="muted small">Looking for suggestions…</span>
            {:else if m.suggestions.length === 0}
              <span class="muted small">No suggestions</span>
            {:else}
              {#each m.suggestions as s (s)}<button type="button" class="ctl" onclick={() => useSuggestion(s)}>{s}</button>{/each}
            {/if}
          </div>
          <div class="cands-row">
            <button type="button" class="ctl" onclick={addToBook}>Add “{m.core}” to this book</button>
          </div>
        {/if}
        {#if m.flags.length}
          <div class="menu-label">Grammar</div>
          {#each m.flags as f, fi (fi)}
            <div class="cands-row wrap">
              <span class="small">{f.message}</span>
              {#if f.fix}<button type="button" class="ctl" onclick={() => applyFix(f.fix!)} title="Change to “{f.fix.text}”">Fix → <span class="serif">{f.fix.text}</span></button>{/if}
            </div>
          {/each}
        {/if}
        <div class="cands-row wrap">
          <button type="button" class="ctl" onclick={editFromMenu}>Edit text</button>
          {#if m.para === "split"}
            <button type="button" class="ctl" onclick={() => setParagraph(true)} title="Start a new paragraph at this word">¶ New paragraph here</button>
          {:else if m.para === "join"}
            <button type="button" class="ctl" onclick={() => setParagraph(false)} title="Join this paragraph to the one before it">Join with paragraph above</button>
          {/if}
          {#if !notes.length && !m.dialect && !m.flags.length && !m.roman}<span class="muted small">{m.inNote ? "This word is in a footnote" : "No footnotes on this page"}</span>{/if}
        </div>
      </div>
    {/if}
    {#if revisit}
      {@const r = revisit}
      {@const now = review.spanById(r.span)?.text ?? ""}
      <div class="cands revisit" style="left:{r.x}px;top:{r.y}px" role="dialog" aria-label="Correction of {now}">
        <div class="cands-row">
          <span class="small">Now <b class="serif">{now || "(removed)"}</b>{#if r.was && r.was !== now} · was <s class="serif">{r.was}</s>{/if}</span>
          <span class="grow"></span>
          <button type="button" class="ctl x" onclick={() => (revisit = null)} aria-label="Close">✕</button>
        </div>
        {#if r.history.length}
          <ul class="hist">
            {#each r.history.slice(0, 4) as h (h.id)}
              <li class:undone={h.undone}>
                <span class="small">{h.label}</span>
                {#if h.undoable && !h.undone}<button type="button" class="link" onclick={() => undoEntry(h.id)}>Undo</button>{/if}
              </li>
            {/each}
          </ul>
        {/if}
        <div class="cands-row wrap">
          {#if r.was && r.was !== now}<button type="button" class="ctl" onclick={() => setReading(r.was!)}>Restore “{r.was}”</button>{/if}
          {#each r.choices.filter((c) => c !== r.was) as c (c)}
            <button type="button" class="ctl" onclick={() => setReading(c)}>Use “{c}”</button>
          {/each}
        </div>
      </div>
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
        <p class="runhead" aria-label="Running head">{@render wtext(headLine)}</p>
      {/if}
      {#each shownBody as p, pi (pi)}
        <p class="para {p.kind}" data-label={p.label}>
          {#each p.words as w (w.span.id)}{@render fnnum(w.span.id)}{#if w.mark.issue && w.mark.kind !== "none"}<button
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
              onkeydown={(e) => onMarkKey(e, w)}>{@render wtext(w.span.text, w.span.id)}</button>{:else if w.mark.kind !== "none"}<mark
              class="mark {w.mark.kind}"
              class:focused={review.selected?.span === w.span.id}
              data-span={w.span.id}
              title={title(w)}>{@render wtext(w.span.text, w.span.id)}</mark>{:else}<span
              data-span={w.span.id}
              class:focused={review.selected?.span === w.span.id}>{@render wtext(w.span.text, w.span.id)}</span>{/if}{@render fnref(w.span.id)}{w.span.trailing}{/each}
        </p>
      {/each}
      {#if foot.length}
        <hr class="divider" />
        {#each foot as p, pi (pi)}
          <p class="para {p.kind}" data-label={p.label}>
            {#each p.words as w (w.span.id)}{@render fnnum(w.span.id)}{#if w.mark.issue && w.mark.kind !== "none"}<button type="button" class="mark {w.mark.kind}" class:focused={review.selected?.span === w.span.id} data-span={w.span.id} aria-label="{w.span.text}: {w.mark.issue.kind.replace('_', ' ')}" title={title(w)} onmouseenter={() => (review.hoverSpan = w.span.id)} onmouseleave={() => (review.hoverSpan = null)} onfocus={() => (review.hoverSpan = w.span.id)} onblur={() => (review.hoverSpan = null)} onclick={() => review.select(w.mark.issue!.id)} onkeydown={(e) => onMarkKey(e, w)}>{@render wtext(w.span.text, w.span.id)}</button>{:else if w.mark.kind !== "none"}<mark class="mark {w.mark.kind}" data-span={w.span.id} title={title(w)}>{@render wtext(w.span.text, w.span.id)}</mark>{:else}<span data-span={w.span.id}>{@render wtext(w.span.text, w.span.id)}</span>{/if}{@render fnref(w.span.id)}{w.span.trailing}{/each}
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
  .hl-menu {
    position: relative;
    flex-shrink: 0;
  }
  .hl-pop {
    position: fixed;
    z-index: 20;
    margin-top: 4px;
    display: grid;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--raised);
    box-shadow: var(--shadow-page);
    font: 13px var(--font-ui);
    color: var(--text);
  }
  .word-pop.above {
    transform: translateY(-100%);
  }
  .menu-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  .notes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
    max-height: 220px;
    overflow-y: auto;
  }
  .note-btn {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 4px;
    cursor: default;
  }
  .note-btn:hover:not(:disabled),
  .note-btn:focus-visible {
    background: var(--accent-bg);
  }
  .note-btn:focus-visible {
    outline: 2px solid var(--accent);
  }
  .note-btn:disabled {
    color: var(--muted);
  }
  .hl-sep {
    height: 1px;
    background: var(--border-soft);
  }
  .hl-pop .small {
    max-width: 220px;
    white-space: normal;
  }
  .gram {
    text-decoration: underline wavy var(--region-header);
    text-decoration-thickness: 1px;
    text-underline-offset: 6px;
    text-decoration-skip-ink: none;
  }
  .misspelt {
    text-decoration: underline wavy var(--danger);
    text-decoration-thickness: 1px;
    text-underline-offset: 3px;
    text-decoration-skip-ink: none;
  }
  .hl-pop label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hl {
    border-radius: 2px;
  }
  .hl.num {
    background: color-mix(in srgb, var(--region-header) 28%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--region-header) 60%, transparent);
  }
  .hl.punct {
    background: color-mix(in srgb, var(--warn) 40%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--warn) 80%, transparent);
  }
  .hl.sym {
    background: color-mix(in srgb, var(--danger) 28%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--danger) 70%, transparent);
  }
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
  /* track corrections: corrected words in a green box */
  .scroller.track .mark.accepted,
  .scroller.track .mark.auto {
    outline: 1.5px solid var(--ok);
    outline-offset: 1px;
    border-radius: 2px;
    background: var(--ok-bg);
    text-decoration: none;
    cursor: pointer;
  }
  .track-btn {
    all: unset;
    flex-shrink: 0;
    padding: 2px 8px;
    border-radius: 5px;
    border: 1px solid var(--border-input);
    font: 500 12px var(--font-ui);
    cursor: default;
    color: var(--text-2);
  }
  .track-btn.on {
    border-color: var(--ok);
    background: var(--ok-bg);
    color: var(--text);
  }
  .track-btn:focus-visible {
    outline: 2px solid var(--accent);
  }
  .cands {
    position: absolute;
    z-index: 4;
    width: min(340px, calc(100% - 32px));
    display: grid;
    gap: 6px;
    padding: 8px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--raised);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.2);
    font-family: var(--font-ui);
    font-size: 12.5px;
  }
  .cands-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .cands-row.wrap {
    flex-wrap: wrap;
  }
  .cands-row.wrap:empty {
    display: none;
  }
  .cands-row input {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
    font: 15px var(--font-serif);
  }
  .cands-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
    max-height: 200px;
    overflow: auto;
  }
  .cands-list button {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 5px;
    cursor: pointer;
  }
  .cands-list button:hover,
  .cands-list button.on {
    background: var(--accent-bg);
  }
  .cands-list button:focus-visible {
    outline: 2px solid var(--accent);
  }
  .cands-list .ctext {
    font: 15px var(--font-serif);
  }
  .ctl.x {
    padding: 2px 7px;
  }
  .hist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }
  .hist li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .hist li.undone {
    text-decoration: line-through;
    color: var(--muted);
  }
  .serif {
    font-family: var(--font-serif);
    font-size: 14px;
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
  .para span[data-span]:hover {
    text-decoration: underline dotted var(--accent);
    text-underline-offset: 3px;
  }
  .fnref,
  .fnnum {
    font: 600 0.62em var(--font-ui);
    color: var(--accent-text);
    padding: 0 1px;
    user-select: none;
  }
  .fnref.sug {
    color: var(--muted);
    border-bottom: 1px dashed currentColor;
  }
  .fnnum {
    margin-right: 3px;
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
