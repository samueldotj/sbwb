// Preview render URLs and zoom quantization (D-14, UX-03).

/** Bitmap pixels per PDF point the backend will render; must match Rust SCALES. */
export const SCALES = [0.2, 0.35, 0.5, 0.75, 1, 1.5, 2, 3, 4, 6] as const;
export const THUMB_SCALE = 0.2;
export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 6;
/** Zoom steps for +/- (CSS px per point). 1 = 72 DPI on a 1x display. */
export const ZOOM_STEPS = [0.25, 0.35, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6] as const;

const isWindows = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

/** URL the webview loads for a page render at a backend scale. */
export function renderUrl(page: number, scale: number): string {
  const base = isWindows ? "http://sbwb-render.localhost" : "sbwb-render://localhost";
  return `${base}/p/${page}?s=${scale}`;
}

/** Smallest backend scale that is at least the CSS zoom times the device pixel ratio. */
export function pickScale(cssPxPerPt: number, dpr = 1): number {
  const need = cssPxPerPt * dpr;
  for (const s of SCALES) if (s >= need - 1e-9) return s;
  return SCALES[SCALES.length - 1]!;
}

export function nextZoom(current: number, dir: 1 | -1): number {
  if (dir === 1) {
    for (const z of ZOOM_STEPS) if (z > current + 1e-9) return z;
    return MAX_ZOOM;
  }
  for (let i = ZOOM_STEPS.length - 1; i >= 0; i--) {
    const z = ZOOM_STEPS[i]!;
    if (z < current - 1e-9) return z;
  }
  return MIN_ZOOM;
}

/** Zoom that fits a page of `wPt x hPt` points into `wPx x hPx` CSS pixels. */
export function fitZoom(wPt: number, hPt: number, wPx: number, hPx: number, mode: "page" | "width" = "page"): number {
  if (wPt <= 0 || hPt <= 0) return 1;
  const zw = wPx / wPt;
  if (mode === "width") return clampZoom(zw);
  return clampZoom(Math.min(zw, hPx / hPt));
}

export function clampZoom(z: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
}
