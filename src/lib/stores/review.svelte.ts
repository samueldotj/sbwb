// Review state (REV-01..06): threshold and filters, the current page's
// effective text and issues, the selected issue, book-wide counts, J/K
// navigation, and decisions. One store so the transcript marks, scan
// highlights, inspector, filmstrip, and rail agree.

import {
  api,
  errorMessage,
  type Decision,
  type Issue,
  type IssueCounts,
  type IssueFilter,
  type IssueKind,
  type PageIssueCounts,
  type PageText,
  type Region,
  type Span,
} from "$lib/api";
import { isTauri, listen } from "$lib/ipc";
import { ui } from "./ui.svelte";
import { view } from "./view.svelte";

export type MarkKind = "issue" | "deferred" | "accepted" | "auto" | "none";

const EMPTY_COUNTS: IssueCounts = {
  unresolved: 0,
  matching: 0,
  above_threshold: 0,
  filtered_kind: 0,
  deferred: 0,
  resolved: 0,
  stale: 0,
  flagged: 0,
  unprocessed_pages: 0,
  pages_in_scope: 0,
};

class ReviewState {
  // ----- filters (REV-02) -----
  threshold = $state(70);
  excludeKinds = $state<IssueKind[]>([]);
  deferredView = $state(false);
  byPriority = $state(false);
  hideCompleted = $state(false);
  hideAuto = $state(false);
  autoAdvance = $state(true);

  // ----- current page -----
  page = $state<number | null>(null);
  spans = $state<Span[]>([]);
  proposals = $state<PageText["proposals"]>([]);
  regions = $state<Region[]>([]);
  issues = $state<Issue[]>([]);
  textRevision = $state(0);
  loading = $state(false);

  // ----- selection and hover -----
  selectedId = $state<string | null>(null);
  hoverSpan = $state<string | null>(null);
  /** Set while the Edit box is open: the draft text (PRJ-04). */
  editing = $state<{ span: string; text: string } | null>(null);

  // ----- book-wide -----
  counts = $state<IssueCounts>(EMPTY_COUNTS);
  pageCounts = $state<Map<number, PageIssueCounts>>(new Map());
  #unlisten: (() => void) | null = null;
  #loadToken = 0;
  #prefsLoaded = false;

  get filter(): IssueFilter {
    return { threshold: this.threshold, exclude_kinds: this.excludeKinds, deferred_view: this.deferredView, by_priority: this.byPriority };
  }

  /** Issues on the current page in the working set, reading order. */
  get matching(): Issue[] {
    const want = this.deferredView ? "deferred" : "open";
    return this.issues.filter((i) => i.status === want && !this.excludeKinds.includes(i.kind) && (i.score === null || i.score < this.threshold));
  }
  get selected(): Issue | null {
    return this.issues.find((i) => i.id === this.selectedId) ?? null;
  }
  get selectedIndex(): number {
    return this.matching.findIndex((i) => i.id === this.selectedId);
  }
  get pageUnresolved(): number {
    return this.issues.filter((i) => i.status === "open" || i.status === "deferred").length;
  }

  spanById(id: string): Span | null {
    return this.spans.find((s) => s.id === id) ?? null;
  }

  /** The mark a span should carry in the transcript and on the scan. */
  markFor(span: Span): { kind: MarkKind; issue: Issue | null } {
    const issue = this.issues.find((i) => i.span === span.id && (i.status === "open" || i.status === "deferred"));
    if (issue) {
      const shown = !this.excludeKinds.includes(issue.kind) && (issue.score === null || issue.score < this.threshold);
      if (issue.status === "deferred") return { kind: this.deferredView && shown ? "deferred" : "none", issue };
      if (shown && !this.deferredView) return { kind: "issue", issue };
      return { kind: "none", issue };
    }
    if (span.origin === "accepted" || span.origin === "manual") return { kind: this.hideCompleted ? "none" : "accepted", issue: null };
    if (span.origin === "auto_applied") return { kind: this.hideAuto ? "none" : "auto", issue: null };
    return { kind: "none", issue: null };
  }

  async init() {
    if (!isTauri) return;
    this.#unlisten = await listen("project:changed", () => void this.refreshCounts());
  }

  async loadPrefs() {
    if (!isTauri || this.#prefsLoaded) return;
    this.#prefsLoaded = true;
    try {
      const p = await api.reviewPrefsGet();
      if (p && typeof p === "object") {
        if (typeof p.threshold === "number") this.threshold = p.threshold;
        if (Array.isArray(p.exclude_kinds)) this.excludeKinds = p.exclude_kinds;
        if (typeof p.by_priority === "boolean") this.byPriority = p.by_priority;
        if (typeof p.auto_advance === "boolean") this.autoAdvance = p.auto_advance;
        if (typeof p.hide_completed === "boolean") this.hideCompleted = p.hide_completed;
        if (typeof p.hide_auto === "boolean") this.hideAuto = p.hide_auto;
      }
    } catch {
      /* defaults */
    }
  }

  savePrefs() {
    if (!isTauri) return;
    void api
      .reviewPrefsSet({
        threshold: this.threshold,
        exclude_kinds: this.excludeKinds,
        by_priority: this.byPriority,
        auto_advance: this.autoAdvance,
        hide_completed: this.hideCompleted,
        hide_auto: this.hideAuto,
      })
      .catch(() => {});
  }

  reset() {
    this.page = null;
    this.spans = [];
    this.proposals = [];
    this.regions = [];
    this.issues = [];
    this.selectedId = null;
    this.editing = null;
    this.counts = EMPTY_COUNTS;
    this.pageCounts = new Map();
    this.#prefsLoaded = false;
  }

  async refreshCounts() {
    if (!isTauri) return;
    try {
      const c = await api.reviewCounts(this.filter);
      this.counts = c.book;
      this.pageCounts = new Map(c.pages.map((p) => [p.page, p]));
    } catch {
      /* no book */
    }
  }

  /** Load the effective text and issues of a page. Keeps the selection
   *  when the issue still exists. */
  async load(index: number) {
    if (!isTauri) {
      this.page = index;
      return;
    }
    const token = ++this.#loadToken;
    this.loading = true;
    try {
      const [text, issues, layout] = await Promise.all([api.pageText(index), api.pageIssues(index), api.pageLayout(index)]);
      if (token !== this.#loadToken) return;
      this.page = index;
      this.spans = text?.spans ?? [];
      this.proposals = text?.proposals ?? [];
      this.textRevision = text?.text_revision ?? 0;
      this.regions = layout?.regions ?? [];
      this.issues = issues;
      if (this.selectedId && !issues.some((i) => i.id === this.selectedId)) this.selectedId = null;
    } catch (e) {
      if (token === this.#loadToken) ui.toast(`Could not load page ${index + 1}: ${errorMessage(e)}`, "error", 6000);
    } finally {
      if (token === this.#loadToken) this.loading = false;
    }
  }

  async reload() {
    if (this.page !== null) await this.load(this.page);
    await this.refreshCounts();
  }

  select(id: string | null) {
    this.selectedId = id;
    if (id) ui.inspectorTab = "issue";
  }

  /** Next / previous matching issue across the book (REV-04). */
  async step(forward: boolean) {
    if (!isTauri) return;
    try {
      const r = await api.nextIssue(this.selectedId, this.selectedId ? null : view.page, forward, this.filter);
      if (!r.issue) {
        const what = this.deferredView ? "deferred items" : "issues";
        ui.toast(this.counts.matching === 0 && !this.deferredView ? `No ${what} match the current threshold and filters` : `No more ${what}`, "info");
        ui.announce("No more issues");
        return;
      }
      if (r.wrapped) ui.toast(forward ? "Wrapped to the first issue" : "Wrapped to the last issue", "info", 1800);
      if (r.issue.page !== view.page) {
        view.goTo(r.issue.page);
        await this.load(r.issue.page);
      }
      this.select(r.issue.id);
      const pos = this.matching.findIndex((i) => i.id === r.issue!.id);
      ui.announce(`Issue ${pos + 1} of ${this.matching.length} on page ${r.issue.page + 1}: ${r.issue.original}`);
    } catch (e) {
      ui.toast(`Could not find the next issue: ${errorMessage(e)}`, "error", 6000);
    }
  }

  /** Record a decision on the selected issue (REV-03). Returns the
   *  outcome so the caller can offer a grouped correction (REV-05). */
  async decide(decision: Decision) {
    const issue = this.selected;
    if (!issue || !isTauri) return null;
    const span = this.spanById(issue.span);
    const revision = span?.revision ?? issue.span_revision;
    try {
      const out = await ui.save(() => api.issueDecide(issue.id, decision, revision));
      this.editing = null;
      const verb = decision.kind === "accept" ? "Accepted" : decision.kind === "edit" ? "Saved" : decision.kind === "skip" ? "Kept" : "Deferred";
      ui.announce(`${verb} ${out.span_text}`);
      await this.load(issue.page);
      void this.refreshCounts();
      if (this.autoAdvance) await this.step(true);
      return out;
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
      if (String(errorMessage(e)).includes("reload")) await this.load(issue.page);
      return null;
    }
  }

  async undo(historyId: string) {
    try {
      await ui.save(() => api.historyUndo(historyId));
      ui.announce("Undone");
      await this.reload();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }

  async approve(index: number, outstanding: number) {
    try {
      await ui.save(() => api.pageApprove(index, outstanding));
      ui.announce(`Page ${index + 1} approved`);
      ui.toast(outstanding > 0 ? `Page ${index + 1} approved with ${outstanding} outstanding` : `Page ${index + 1} approved`, "ok");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }

  async unapprove(index: number) {
    try {
      await ui.save(() => api.pageUnapprove(index));
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }

  async flag(spanId: string, note?: string) {
    try {
      const issue = await ui.save(() => api.spanFlag(spanId, note));
      await this.reload();
      this.select(issue.id);
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }

  destroy() {
    this.#unlisten?.();
  }
}

export const review = new ReviewState();
