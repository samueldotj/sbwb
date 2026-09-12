// Typed wrappers over the Tauri commands. Types mirror the Rust structs.

import { invoke } from "./ipc";

export type PageStatus = "unprocessed" | "excluded" | "queued" | "running" | "failed" | "done";

export type PageRow = {
  index: number;
  width_pt: number | null;
  height_pt: number | null;
  status: PageStatus;
  printed_label: string | null;
  error: string | null;
  approved_revision: number | null;
};

export type PageCounts = {
  source: number;
  in_scope: number;
  unprocessed: number;
  excluded: number;
  queued: number;
  running: number;
  failed: number;
  done: number;
  approved: number;
};

export type SourceInfo = {
  name: string;
  size: number;
  blake3: string;
  page_count: number;
  title: string | null;
  author: string | null;
};

export type ProjectMeta = {
  id: string;
  created_at: string;
  app_version: string;
  title: string | null;
  author: string | null;
  source: SourceInfo;
  scope: { ranges: [number, number][] };
  settings: unknown;
};

export type ProjectSummary = {
  path: string;
  meta: ProjectMeta;
  read_only: boolean;
  pages: PageRow[];
  counts: PageCounts;
};

export type SourceCheck = {
  path: string;
  name: string;
  size: number;
  page_count: number;
  encrypted: boolean;
  title: string | null;
  author: string | null;
  blake3: string;
  scan_dpi: number | null;
  has_text_layer: boolean;
  suggested_project: string;
  free_space: number | null;
  warnings: string[];
};

export type OcrWord = {
  block: number;
  paragraph: number;
  line: number;
  index: number;
  bbox: { x: number; y: number; w: number; h: number };
  bbox_px: { x: number; y: number; w: number; h: number };
  confidence: number;
  text: string;
};

export type OcrLine = { block: number; paragraph: number; line: number; bbox: { x: number; y: number; w: number; h: number }; word_count: number };

export type PageOcr = { run_id: string; words: OcrWord[]; lines: OcrLine[]; mean_confidence: number };

export type RunRecord = {
  id: string;
  stage: string;
  page: number | null;
  engine: string | null;
  model: string | null;
  settings: unknown;
  started_at: string;
  finished_at: string | null;
  status: string;
  error: string | null;
  elapsed_ms: number | null;
};

export type ProcessingSettings = {
  model: "eng_best" | "eng_fast";
  dpi: number;
  deskew: boolean;
  despeckle: boolean;
  auto_apply_threshold: number;
  run_stages_automatically: boolean;
  workers: number;
};

export type PackReport = {
  entry: { pack: string; label: string; language: string; file: string; sha256: string; size: number; license: string; url: string };
  path: string;
  status: "missing" | "verified" | { corrupt: { actual_sha256: string } };
};

export type StorageInfo = { cache_dir: string | null; cache_bytes: number; log_dir: string; tessdata_dir: string; tesseract_version: string };

export type PipelineStatusView = {
  active: boolean;
  status: {
    state: "idle" | "running" | "paused" | "stopping" | "done";
    stages: { stage: string; total: number; done: number; failed: number; running: number; elapsed_ms: number; eta_ms: number | null; secs_per_unit: number | null }[];
    activity: string;
    workers: number;
  } | null;
};

export type CommandError = { code: string; message: string };

export function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  if (isCommandError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}

export const api = {
  appInfo: () => invoke<{ version: string; identifier: string; tesseract: string }>("app_info"),
  inspectSource: (path: string, password?: string) =>
    invoke<SourceCheck>("inspect_source", { path, password: password ?? null }),
  importPdf: (req: { source: string; project_path?: string; password?: string; first_pages?: number | null }) =>
    invoke<ProjectSummary>("import_pdf", { req }),
  openProject: (path: string) => invoke<{ summary: ProjectSummary; locked_by: string | null }>("open_project", { path }),
  projectSummary: () => invoke<ProjectSummary | null>("project_summary"),
  closeProject: () => invoke<void>("close_project"),
  saveCopy: (dest: string) => invoke<void>("save_copy", { dest }),
  setScope: (ranges: [number, number][]) => invoke<ProjectSummary>("set_scope", { req: { ranges } }),
  pageOcr: (index: number) => invoke<PageOcr | null>("page_ocr", { index }),
  pageRuns: (index: number) => invoke<RunRecord[]>("page_runs", { index }),
  prefetchRender: (index: number, scale: number) => invoke<void>("prefetch_render", { index, scale }),
  pipelineStart: () => invoke<unknown>("pipeline_start"),
  pipelinePause: () => invoke<void>("pipeline_pause"),
  pipelineResume: () => invoke<void>("pipeline_resume"),
  pipelineCancel: () => invoke<void>("pipeline_cancel"),
  pipelineRetryFailed: () => invoke<unknown>("pipeline_retry_failed"),
  pipelineStatus: () => invoke<PipelineStatusView>("pipeline_status"),
  settingsGet: () => invoke<ProcessingSettings>("settings_get"),
  settingsPreview: (settings: ProcessingSettings) =>
    invoke<{ rerun_pages: number; approved_untouched: number; effective_workers: number }>("settings_preview", { settings }),
  settingsSet: (settings: ProcessingSettings, rerun: boolean) => invoke<number>("settings_set", { settings, rerun }),
  modelsReport: () => invoke<PackReport[]>("models_report"),
  storageInfo: () => invoke<StorageInfo>("storage_info"),
  clearRenderCache: () => invoke<number>("clear_render_cache"),
};

/// "1-50, 60-70" -> [[1,50],[60,70]]; returns null when unparsable.
export function parseRanges(text: string): [number, number][] | null {
  const out: [number, number][] = [];
  for (const part of text.split(/[,;]/)) {
    const t = part.trim();
    if (!t) continue;
    const m = /^(\d+)\s*(?:[-–—]\s*(\d+))?$/.exec(t);
    if (!m) return null;
    const a = Number(m[1]);
    const b = m[2] ? Number(m[2]) : a;
    if (a < 1 || b < a) return null;
    out.push([a, b]);
  }
  return out.length ? out : null;
}

export function scopeLabel(ranges: [number, number][]): string {
  return ranges.map(([a, b]) => (a === b ? `${a}` : `${a}–${b}`)).join(", ");
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(0)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
