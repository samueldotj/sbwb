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
  ocr_done: boolean;
  layout_done: boolean;
  text_done: boolean;
  layout_revision: number;
  text_revision: number;
  approved_outstanding: number;
  approved_at: string | null;
  approval: "none" | "current" | "outdated";
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
  ocr_done: number;
  layout_done: number;
  text_done: number;
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
  language: "modern" | "early_modern";
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

export type RegionKind =
  | "body" | "heading" | "header" | "footer" | "marginalia" | "footnote" | "page_number" | "catchword" | "illustration" | "table" | "uncertain" | "ignore";
export type WordStructure = "text" | "heading1" | "heading2" | "heading3" | "table_cell";

export type Region = {
  id: string;
  kind: RegionKind;
  bbox: { x: number; y: number; w: number; h: number };
  order: number;
  structure: WordStructure;
  column: number | null;
  score: number;
  manual: boolean;
  line_count: number;
  anchor_y: number | null;
};

export type PageLayout = {
  page: number;
  regions: Region[];
  report: Record<string, unknown>;
  algorithm: string | null;
  manual: boolean;
  revision: number;
};

export type SpanOrigin = "ocr" | "auto_applied" | "accepted" | "manual" | "inserted";
export type Anchor = { run: string; index: number; bbox: { x: number; y: number; w: number; h: number }; confidence: number; text: string };
export type Span = {
  id: string;
  page: number;
  seq: number;
  region: string | null;
  text: string;
  anchors: Anchor[];
  origin: SpanOrigin;
  confidence: number | null;
  trailing: string;
  revision: number;
  protected: boolean;
  structure: WordStructure;
  paragraph_start: boolean;
};
export type ProposalStatus = "open" | "applied_auto" | "accepted" | "rejected" | "deferred" | "stale";
export type ProposalKind = "hyphen_join" | "ocr_confusion" | "spelling" | "proper_name";
export type StoredProposal = {
  id: string;
  page: number;
  span: string;
  span_revision: number;
  kind: ProposalKind;
  original: string;
  replacement: string;
  score: number;
  reason: string;
  source: string;
  status: ProposalStatus;
  merged_span: string | null;
  cross_page: boolean;
};
export type PageText = { page: number; spans: Span[]; proposals: StoredProposal[]; text_revision: number };
export type TextPassSummary = {
  applied: number;
  suggested: number;
  accepted: number;
  rejected: number;
  deferred: number;
  stale: number;
  pages_done: number;
  pages_in_scope: number;
  last_run: string | null;
};

// ----- review (M6) -----
export type IssueKind = "missing_text" | "order_ambiguity" | "clipping" | "conflicting_readings" | "risky_substitution" | "questionable_join" | "user_flag";
export type IssueStatus = "open" | "resolved" | "deferred" | "stale";
export type Candidate = { text: string; score: number | null; source: string; proposal: string | null };
export type Issue = {
  id: string;
  page: number;
  seq: number;
  span: string;
  span_revision: number;
  kind: IssueKind;
  score: number | null;
  score_source: string;
  priority: number;
  proposal: string | null;
  original: string;
  replacement: string | null;
  reason: string;
  status: IssueStatus;
  decision: string | null;
  note: string | null;
  candidates: Candidate[];
  bbox: { x: number; y: number; w: number; h: number } | null;
  region: string | null;
};
export type IssueFilter = { threshold: number; exclude_kinds: IssueKind[]; deferred_view: boolean; by_priority: boolean };
export type IssueCounts = {
  unresolved: number;
  matching: number;
  above_threshold: number;
  filtered_kind: number;
  deferred: number;
  resolved: number;
  stale: number;
  flagged: number;
  unprocessed_pages: number;
  pages_in_scope: number;
};
export type PageIssueCounts = { page: number; open: number; matching: number; deferred: number };
export type ReviewCounts = { book: IssueCounts; pages: PageIssueCounts[] };
export type NextIssue = { issue: Issue | null; wrapped: boolean };
export type Decision = { kind: "accept"; text: string } | { kind: "edit"; text: string } | { kind: "skip" } | { kind: "later" };
export type DecisionOutcome = { issue: Issue; history: string; span_text: string; span_revision: number };
export type GroupMatch = { span: string; page: number; seq: number; revision: number; text: string; replacement: string; context: string; conflict: string | null };
export type GroupOutcome = { applied: number; history: string | null; stale: string[] };
export type HistoryEntry = { id: string; ts: string; kind: string; label: string; undone: boolean; undoable: boolean; page: number | null };
export type Draft = { span: string; page: number; text: string; updated_at: string };

export const ISSUE_KIND_LABEL: Record<IssueKind, string> = {
  missing_text: "missing text",
  order_ambiguity: "reading order",
  clipping: "clipping",
  conflicting_readings: "uncertain reading",
  risky_substitution: "risky substitution",
  questionable_join: "questionable join",
  user_flag: "flagged",
};

// ----- export (M7) -----
export type CopyKind = "working" | "clean";
export type PageStructure = "mirror" | "continuous";
export type FurniturePolicy = "styled_paragraphs" | "native_headers_footers";
export type TablePolicy = "text" | "image" | "skip";
export type ExportSettings = {
  copy: CopyKind;
  structure: PageStructure;
  furniture: FurniturePolicy;
  include: { marginalia: boolean; footnotes: boolean; page_numbers: boolean; catchwords: boolean; illustrations: boolean; uncertain: boolean; tables: TablePolicy };
  flag_threshold: number;
  archive: boolean;
  preset: {
    name: string;
    paper: "a4" | "letter" | "custom";
    custom_width_pt: number;
    custom_height_pt: number;
    margin_top_mm: number;
    margin_bottom_mm: number;
    margin_left_mm: number;
    margin_right_mm: number;
    body_font: string;
    body_size_pt: number;
    paragraph_spacing_pt: number;
    line_spacing: number;
    heading_font: string;
    note_size_pt: number;
    drop_cap: { enabled: boolean; lines: number };
  };
  metadata: { title: string | null; author: string | null; subject: string | null };
};
export type ExportRecord = { id: string; ts: string; kind: string; path: string; checksum: string | null; report: unknown };
export type ExportEntry = { id: string; ts: string; kind: string; path: string; name: string; exists: boolean; pages: number; archive: string | null };
export type Readiness = {
  pages_in_scope: number;
  pages_indexed: number;
  pages_approved: number;
  pages_left: number;
  unresolved_below_threshold: number;
  deferred: number;
  ai_pending: number;
  clean_ready: boolean;
  previous: ExportRecord[];
  default_name: string;
  book_title: string | null;
  book_author: string | null;
  pdf_title: string | null;
  pdf_author: string | null;
};
export type Exclusion = { page: number; region: string | null; kind: string; words: number; why: string };
export type PlanStats = { pages: number; paragraphs: number; words: number; running_heads: number; page_numbers: number; side_notes: number; footer_notes: number; footnotes: number; headings: number; images: number; tables: number; flags: number; cross_page_words: number; unanchored_words: number };
export type ExportPreview = { explanation: string; exclusions: Exclusion[]; excluded_words: number; warnings: string[]; stats: PlanStats; clean_blockers: number[] };
export type ExportReport = {
  id: string;
  copy: CopyKind;
  stats: PlanStats;
  exclusions: Exclusion[];
  warnings: string[];
  comments: number;
  highlights: number;
  images: number;
  checksum_blake3: string;
  docx_path: string;
  archive_path: string | null;
  validation: { ok: boolean; checks: { name: string; ok: boolean; detail: string }[] };
  elapsed_ms: number;
};

// ----- targeted refinement (M8) -----
export type MergeOutcome = { words: number; proposals_added: number; spans_inserted: number; agreements: number; second_engine_disagreements: number };
export type RegionOcrResult = {
  outcome: MergeOutcome;
  profile: Record<string, unknown>;
  render: string | null;
  run: string;
  elapsed_ms: number;
  second_engine: string | null;
  raw_text: string;
  second_text: string | null;
};

// ----- AI proofreading (Phase 2) -----
export type ModelInfo = { id: string; label: string };
export type AiProviderStatus = { provider: string; label: string; key_help: string; session_key: boolean; remembered: boolean };
export type AiRunRecord = {
  id: string;
  ts: string;
  provider: string;
  model: string;
  pages: number[];
  consent: unknown;
  requests: number;
  chars_sent: number;
  input_tokens: number | null;
  output_tokens: number | null;
  suggestions: number;
  rejected: number;
  status: string;
  error: string | null;
  elapsed_ms: number | null;
};
export type AiStatus = { providers: AiProviderStatus[]; open_suggestions: number; last_run: AiRunRecord | null };
export type Estimate = { pages: number[]; words: number; chars: number; approx_tokens: number; requests: number; payload_kinds: string[]; cost: string };

// ----- book queue (PRJ-06) -----
export type QueueItem = {
  id: string;
  source: string;
  project_path: string;
  first_pages: number | null;
  settings: ProcessingSettings;
  status: "pending" | "running" | "done" | "failed" | "interrupted";
  error: string | null;
  done: number;
  total: number;
  added_at: string;
};
export type QueueView = { items: QueueItem[]; running: boolean };

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
  pageLayout: (index: number) => invoke<PageLayout | null>("page_layout", { index }),
  layoutSave: (index: number, regions: Region[]) => invoke<PageLayout>("layout_save", { index, regions }),
  layoutRerun: (index: number) => invoke<void>("layout_rerun", { index }),
  pageText: (index: number) => invoke<PageText | null>("page_text", { index }),
  textPassSummary: () => invoke<TextPassSummary | null>("text_pass_summary"),
  textPassRerun: (index?: number) => invoke<number>("text_pass_rerun", { index: index ?? null }),
  vocabList: () => invoke<[string, string][]>("vocab_list"),
  vocabAdd: (word: string, kind: "vocab" | "protected") => invoke<void>("vocab_add", { word, kind }),
  vocabRemove: (word: string) => invoke<void>("vocab_remove", { word }),
  reviewCounts: (filter: IssueFilter) => invoke<ReviewCounts>("review_counts", { filter }),
  pageIssues: (index: number) => invoke<Issue[]>("page_issues", { index }),
  nextIssue: (from: string | null, fromPage: number | null, forward: boolean, filter: IssueFilter) => invoke<NextIssue>("next_issue", { from, fromPage, forward, filter }),
  issueDecide: (id: string, decision: Decision, spanRevision: number) => invoke<DecisionOutcome>("issue_decide", { id, decision, spanRevision }),
  spanFlag: (span: string, note?: string) => invoke<Issue>("span_flag", { span, note: note ?? null }),
  flagRemove: (id: string) => invoke<void>("flag_remove", { id }),
  pageApprove: (index: number, acknowledged: number) => invoke<void>("page_approve", { index, acknowledged }),
  pageUnapprove: (index: number) => invoke<void>("page_unapprove", { index }),
  groupPreview: (original: string, replacement: string) => invoke<GroupMatch[]>("group_preview", { original, replacement }),
  groupApply: (original: string, replacement: string, items: { span: string; revision: number }[]) =>
    invoke<GroupOutcome>("group_apply", { original, replacement, items }),
  historyList: (opts: { limit?: number; span?: string } = {}) => invoke<HistoryEntry[]>("history_list", { limit: opts.limit ?? null, span: opts.span ?? null }),
  historyUndo: (id: string) => invoke<void>("history_undo", { id }),
  draftPut: (span: string, text: string) => invoke<void>("draft_put", { span, text }),
  draftDelete: (span: string) => invoke<void>("draft_delete", { span }),
  draftList: (index?: number) => invoke<Draft[]>("draft_list", { index: index ?? null }),
  reviewPrefsGet: () => invoke<Partial<IssueFilter> & { auto_advance?: boolean; hide_completed?: boolean; hide_auto?: boolean } | null>("review_prefs_get"),
  reviewPrefsSet: (prefs: unknown) => invoke<void>("review_prefs_set", { prefs }),
  exportReadiness: () => invoke<Readiness>("export_readiness"),
  exportPreview: (settings: ExportSettings) => invoke<ExportPreview>("export_preview", { settings }),
  exportRun: (settings: ExportSettings, dest: string) => invoke<void>("export_run", { settings, dest }),
  exportCancel: () => invoke<void>("export_cancel"),
  exportDefaultsGet: () => invoke<ExportSettings>("export_defaults_get"),
  exportDefaultsSet: (settings: ExportSettings) => invoke<void>("export_defaults_set", { settings }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openFile: (path: string) => invoke<void>("open_file", { path }),
  exportsList: () => invoke<ExportEntry[]>("exports_list"),
  regionOcr: (index: number, bbox: { x: number; y: number; w: number; h: number }, opts: { enlarge?: number; secondEngine?: boolean; region?: string } = {}) =>
    invoke<RegionOcrResult>("region_ocr", { index, bbox, enlarge: opts.enlarge ?? 3, secondEngine: opts.secondEngine ?? false, region: opts.region ?? null }),
  secondEngineAvailable: () => invoke<boolean>("second_engine_available"),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiKeySet: (provider: string, key: string, remember: boolean) => invoke<boolean>("ai_key_set", { provider, key, remember }),
  aiKeyForget: (provider: string) => invoke<void>("ai_key_forget", { provider }),
  aiModels: (provider: string) => invoke<ModelInfo[]>("ai_models", { provider }),
  aiEstimate: (pages: number[]) => invoke<Estimate>("ai_estimate", { pages }),
  aiRun: (req: { provider: string; model: string; pages: number[]; max_requests: number; max_input_chars: number; consent: boolean }) => invoke<string>("ai_run", { req }),
  aiCancel: () => invoke<void>("ai_cancel"),
  aiRuns: () => invoke<AiRunRecord[]>("ai_runs"),
  queueList: () => invoke<QueueView>("queue_list"),
  queueAdd: (req: { sources: string[]; dest_dir: string | null; first_pages: number | null; settings: ProcessingSettings }) => invoke<QueueView>("queue_add", { req }),
  queueRemove: (id: string) => invoke<QueueView>("queue_remove", { id }),
  queueMove: (id: string, delta: number) => invoke<QueueView>("queue_move", { id, delta }),
  queueRetry: (id: string) => invoke<QueueView>("queue_retry", { id }),
  queueClearFinished: () => invoke<QueueView>("queue_clear_finished"),
  queueStart: () => invoke<void>("queue_start"),
  queueStop: () => invoke<void>("queue_stop"),
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
