// Layout mode state (LAY-02): the regions of the current page, editing
// tools, an in-memory undo stack, and save/revert/rerun.

import { api, errorMessage, type Region, type RegionKind, type WordStructure } from "$lib/api";
import { isTauri } from "$lib/ipc";
import { ui } from "./ui.svelte";

export type Tool = "select" | "draw" | "split" | "merge";

export const KIND_LABEL: Record<RegionKind, string> = {
  body: "body",
  heading: "heading",
  header: "header",
  footer: "footer",
  marginalia: "marginalia",
  footnote: "footnote",
  page_number: "page no.",
  catchword: "catchword",
  illustration: "illustration",
  table: "table",
  uncertain: "uncertain",
  ignore: "ignore",
};

export const KIND_PLACEMENT: Record<RegionKind, string> = {
  body: "→ body",
  heading: "→ body",
  header: "→ running head",
  footer: "→ footer",
  marginalia: "→ side note",
  footnote: "→ footnote",
  page_number: "→ page number",
  catchword: "archive only",
  illustration: "→ image",
  table: "→ body",
  uncertain: "review",
  ignore: "excluded",
};

/// Placement under the native headers/footers policy (D-12).
export const KIND_PLACEMENT_NATIVE: Record<RegionKind, string> = {
  ...KIND_PLACEMENT,
  header: "→ Word header",
  page_number: "→ Word header",
  footer: "→ Word footer",
  marginalia: "→ Word footer",
};

/// The furniture policy the book exports with; loaded from the export
/// defaults so Layout mode previews the effect per page.
export const exportPolicy = $state({ native: false });

const RANK: Record<RegionKind, number> = {
  header: 0,
  page_number: 1,
  heading: 2,
  body: 2,
  table: 2,
  illustration: 2,
  marginalia: 3,
  footnote: 4,
  footer: 5,
  catchword: 6,
  uncertain: 7,
  ignore: 8,
};

function newId(): string {
  return crypto.randomUUID();
}

class LayoutState {
  page = $state<number | null>(null);
  regions = $state<Region[]>([]);
  saved = $state<Region[]>([]);
  report = $state<Record<string, unknown> | null>(null);
  manual = $state(false);
  revision = $state(0);
  loading = $state(false);
  selectedId = $state<string | null>(null);
  tool = $state<Tool>("select");
  showRegions = $state(true);
  #undo: Region[][] = [];

  get dirty(): boolean {
    return JSON.stringify(this.regions) !== JSON.stringify(this.saved);
  }
  get selected(): Region | null {
    return this.regions.find((r) => r.id === this.selectedId) ?? null;
  }
  get canUndo(): boolean {
    return this.#undo.length > 0;
  }

  async load(page: number) {
    this.page = page;
    this.selectedId = null;
    this.#undo = [];
    if (!isTauri) {
      this.regions = [];
      this.saved = [];
      return;
    }
    this.loading = true;
    try {
      const l = await api.pageLayout(page);
      this.regions = l ? structuredClone(l.regions) : [];
      this.saved = l ? structuredClone(l.regions) : [];
      this.report = (l?.report as Record<string, unknown>) ?? null;
      this.manual = l?.manual ?? false;
      this.revision = l?.revision ?? 0;
    } catch (e) {
      ui.toast(errorMessage(e), "error");
    } finally {
      this.loading = false;
    }
  }

  #push() {
    this.#undo.push(structuredClone($state.snapshot(this.regions)));
    if (this.#undo.length > 50) this.#undo.shift();
  }

  undo() {
    const prev = this.#undo.pop();
    if (prev) this.regions = prev;
  }

  #renumber() {
    this.regions = this.regions.map((r, i) => ({ ...r, order: i }));
  }

  /// Re-derive order from class rank then position (used after kind changes).
  autoOrder() {
    this.#push();
    const sorted = [...this.regions].sort(
      (a, b) => RANK[a.kind] - RANK[b.kind] || (a.column ?? 0) - (b.column ?? 0) || a.bbox.y - b.bbox.y || a.bbox.x - b.bbox.x,
    );
    this.regions = sorted;
    this.#renumber();
  }

  setKind(id: string, kind: RegionKind) {
    this.#push();
    this.regions = this.regions.map((r) => (r.id === id ? { ...r, kind, manual: true } : r));
  }
  setStructure(id: string, structure: WordStructure) {
    this.#push();
    this.regions = this.regions.map((r) => (r.id === id ? { ...r, structure, manual: true } : r));
  }
  setBox(id: string, bbox: Region["bbox"], pushUndo = true) {
    if (pushUndo) this.#push();
    this.regions = this.regions.map((r) => (r.id === id ? { ...r, bbox, manual: true } : r));
  }
  add(bbox: Region["bbox"], kind: RegionKind = "uncertain"): Region {
    this.#push();
    const r: Region = {
      id: newId(),
      kind,
      bbox,
      order: this.regions.length,
      structure: "text",
      column: null,
      score: 1,
      manual: true,
      line_count: 0,
      anchor_y: null,
    };
    this.regions = [...this.regions, r];
    this.selectedId = r.id;
    return r;
  }
  remove(id: string) {
    this.#push();
    this.regions = this.regions.filter((r) => r.id !== id);
    this.#renumber();
    if (this.selectedId === id) this.selectedId = null;
  }
  /// Split a region horizontally at a page-space y.
  splitAt(id: string, y: number) {
    const r = this.regions.find((x) => x.id === id);
    if (!r || y <= r.bbox.y + 2 || y >= r.bbox.y + r.bbox.h - 2) return;
    this.#push();
    const top: Region = { ...r, bbox: { ...r.bbox, h: y - r.bbox.y }, manual: true };
    const bottom: Region = { ...r, id: newId(), bbox: { ...r.bbox, y, h: r.bbox.y + r.bbox.h - y }, manual: true, line_count: 0 };
    const i = this.regions.findIndex((x) => x.id === id);
    const next = [...this.regions];
    next.splice(i, 1, top, bottom);
    this.regions = next;
    this.#renumber();
    this.selectedId = bottom.id;
  }
  merge(idA: string, idB: string) {
    const a = this.regions.find((x) => x.id === idA);
    const b = this.regions.find((x) => x.id === idB);
    if (!a || !b || idA === idB) return;
    this.#push();
    const x0 = Math.min(a.bbox.x, b.bbox.x);
    const y0 = Math.min(a.bbox.y, b.bbox.y);
    const x1 = Math.max(a.bbox.x + a.bbox.w, b.bbox.x + b.bbox.w);
    const y1 = Math.max(a.bbox.y + a.bbox.h, b.bbox.y + b.bbox.h);
    const merged: Region = { ...a, bbox: { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }, manual: true, line_count: a.line_count + b.line_count };
    this.regions = this.regions.filter((x) => x.id !== idB).map((x) => (x.id === idA ? merged : x));
    this.#renumber();
    this.selectedId = merged.id;
  }
  mergeWithNext(id: string) {
    const i = this.regions.findIndex((x) => x.id === id);
    const next = this.regions[i + 1];
    if (next) this.merge(id, next.id);
  }
  reorder(ids: string[]) {
    this.#push();
    const byId = new Map(this.regions.map((r) => [r.id, r]));
    this.regions = ids.map((id) => byId.get(id)!).filter(Boolean);
    this.#renumber();
  }

  async save() {
    if (this.page === null) return;
    try {
      const l = await api.layoutSave(this.page, $state.snapshot(this.regions));
      this.regions = structuredClone(l.regions);
      this.saved = structuredClone(l.regions);
      this.manual = l.manual;
      this.revision = l.revision;
      this.#undo = [];
      ui.toast("Layout saved", "ok");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  revert() {
    this.regions = structuredClone($state.snapshot(this.saved));
    this.#undo = [];
    this.selectedId = null;
  }
  async rerun() {
    if (this.page === null) return;
    try {
      await api.layoutRerun(this.page);
      ui.toast("Re-analysing the page layout", "info");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
}

export const layout = new LayoutState();
