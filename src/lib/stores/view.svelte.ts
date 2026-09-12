// Workspace view state: mode, current page, zoom (UX-03). Shared so the
// keyboard handler, filmstrip, grid, and scan pane agree.

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
    this.goTo(this.page + 1);
  }
  prev() {
    this.goTo(this.page - 1);
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
