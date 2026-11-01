// Workspace view state: mode, current page, zoom (UX-03). Shared so the
// keyboard handler, filmstrip, grid, and scan pane agree.

import { nearestUnreviewed } from "$lib/pagefilter";
import { project } from "./project.svelte";
import { ui } from "./ui.svelte";

export type Mode = "processing" | "review" | "layout";
export type ZoomMode = "fit" | "width" | "custom";

class ViewState {
  mode = $state<Mode>("processing");
  /** Zero-based physical page index. */
  page = $state(0);
  pageCount = $state(0);
  zoomMode = $state<ZoomMode>("fit");
  /** CSS px per PDF point when zoomMode is custom. */
  zoom = $state(1);
  /** Whether a render for the requested page is still loading. */
  loading = $state(false);
  /** Backend render scale the scan pane is asking for; neighbours are prefetched at it. */
  renderScale = $state(0);

  reset(pageCount: number) {
    this.pageCount = pageCount;
    this.page = 0;
    this.mode = "processing";
    this.zoomMode = "fit";
    this.zoom = 1;
  }

  goTo(index: number) {
    if (this.pageCount === 0) return;
    this.page = Math.min(this.pageCount - 1, Math.max(0, index));
  }
  next() {
    this.#step(1);
  }
  prev() {
    this.#step(-1);
  }
  /** One page on; with the unreviewed filter on, the next page the filter lists.
   *  Layout mode lists no pages, so there every page is a step. */
  #step(dir: -1 | 1) {
    if (!ui.unreviewedOnly || this.mode === "layout" || !project.summary) {
      this.goTo(this.page + dir);
      return;
    }
    const to = nearestUnreviewed(project.summary.pages, this.page, dir);
    if (to === null) ui.toast(`No unreviewed pages ${dir > 0 ? "after" : "before"} page ${this.page + 1}`, "info");
    else this.goTo(to);
  }
  open(index: number) {
    this.goTo(index);
    this.mode = "review";
  }
  editLayout() {
    this.mode = "layout";
  }
}

export const view = new ViewState();
