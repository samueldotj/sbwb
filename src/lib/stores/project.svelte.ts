// Current book: summary, busy state, and actions (PRJ-01..03).

import { api, errorMessage, type ProjectSummary } from "$lib/api";
import { isTauri, listen } from "$lib/ipc";
import { recent } from "$lib/recent";
import { ui } from "./ui.svelte";

class ProjectState {
  summary = $state<ProjectSummary | null>(null);
  busy = $state<string | null>(null);
  lockedBy = $state<string | null>(null);
  #unlisten: (() => void) | null = null;

  get isOpen() {
    return this.summary !== null;
  }

  get title(): string {
    const m = this.summary?.meta;
    if (!m) return "";
    return m.title ?? m.source.title ?? m.source.name.replace(/\.pdf$/i, "");
  }

  get scopeLabel(): string {
    const s = this.summary;
    if (!s) return "";
    const c = s.counts;
    return c.in_scope === c.source ? `${c.source} pp` : `${c.in_scope} of ${c.source} pp`;
  }

  async init() {
    if (!isTauri) {
      // Dev-only layout mock: http://localhost:1420/?mock=review
      if (import.meta.env.DEV && new URLSearchParams(location.search).has("mock")) this.summary = mockSummary(110, 50);
      return;
    }
    this.#unlisten = await listen("project:changed", () => void this.refresh());
    await this.refresh();
  }

  async refresh() {
    try {
      this.summary = await api.projectSummary();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 5000);
    }
  }

  async importPdf(source: string, opts: { firstPages?: number | null; projectPath?: string; password?: string } = {}) {
    this.busy = "Importing…";
    try {
      const s = await api.importPdf({
        source,
        first_pages: opts.firstPages === undefined ? 50 : opts.firstPages,
        project_path: opts.projectPath,
        password: opts.password,
      });
      this.summary = s;
      this.lockedBy = null;
      await recent.remember(s);
      ui.toast(`Imported ${s.meta.source.page_count} pages`, "ok");
      return s;
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
      throw e;
    } finally {
      this.busy = null;
    }
  }

  async open(path: string) {
    this.busy = "Opening…";
    try {
      const r = await api.openProject(path);
      this.summary = r.summary;
      this.lockedBy = r.locked_by;
      await recent.remember(r.summary);
      if (r.locked_by) ui.toast(`Opened read-only: in use by ${r.locked_by}`, "warn", 6000);
      return r.summary;
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
      throw e;
    } finally {
      this.busy = null;
    }
  }

  async close() {
    if (this.summary) await recent.remember(this.summary);
    await api.closeProject();
    this.summary = null;
    this.lockedBy = null;
  }

  async setScope(ranges: [number, number][]) {
    try {
      this.summary = await api.setScope(ranges);
      await recent.remember(this.summary);
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
      throw e;
    }
  }

  async saveCopy(dest: string) {
    try {
      await api.saveCopy(dest);
      ui.toast("Copy saved", "ok");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }

  destroy() {
    this.#unlisten?.();
  }
}

function mockSummary(pages: number, scope: number): ProjectSummary {
  const rows = Array.from({ length: pages }, (_, i) => ({
    index: i,
    width_pt: 361,
    height_pt: 605,
    status: (i < scope ? (i < 10 ? "done" : "queued") : "unprocessed") as ProjectSummary["pages"][number]["status"],
    printed_label: null,
    error: null,
    approved_revision: i < 3 ? 1 : null,
    ocr_done: i < 10,
    layout_done: i < 10,
    text_done: false,
    layout_revision: 0,
    text_revision: 0,
  }));
  return {
    path: "C:\mock\book.sbwb",
    read_only: false,
    meta: {
      id: "mock",
      created_at: new Date().toISOString(),
      app_version: "0.1.0",
      title: "The History of Christianity in India",
      author: "James Hough",
      source: { name: "book.pdf", size: 15_534_508, blake3: "0".repeat(64), page_count: pages, title: null, author: null },
      scope: { ranges: [[1, scope]] },
      settings: {},
    },
    pages: rows,
    counts: { source: pages, in_scope: scope, unprocessed: pages - scope, excluded: 0, queued: scope - 10, running: 0, failed: 0, done: 10, approved: 3, ocr_done: 10, layout_done: 10, text_done: 0 },
  };
}

export const project = new ProjectState();
