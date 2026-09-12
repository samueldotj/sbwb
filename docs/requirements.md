# SBWB Product Requirements

## 0. Status and amendments

This is the working requirements document for SBWB. It began as an AI summary
of a manually written canonical document; where the two conflict, the manual
document wins and the conflict is recorded in [decisions.md](decisions.md).

Amendments applied on 2026-09-12 from settled decisions:

| Decision | Change in this document |
| --- | --- |
| D-01 | Implementation is Rust with a Tauri 2 shell (section 2.1). |
| D-02 | Windows 11 x64 is the only qualified platform for the first release; macOS and Linux builds must keep compiling (sections 2.1, NFR-01). |
| D-03 | PaddleOCR removed from OCR-01 and section 12. |
| D-05 | Section 8 (optional AI reconstruction) is Phase 2. |
| D-06 | NFR-02 names the Hough 1839 fixture as the reference corpus. |
| D-10 | REV-07 dropped; ID reserved (section 7, section 12). |
| D-11 | LAY-01 names classical image analysis as the primary layout source and Tesseract segmentation as extra evidence. |
| D-12 | EXP-02, EXP-03, and A-12 changed: styled body paragraphs are the default placement for page furniture; native headers/footers are an option. |
| D-13 | Frontend is Svelte 5 + TypeScript + Vite (section 2.1). |
| D-14 | IMG-01 and UX-03: previews are PDFium renders cached by the backend; zoom re-renders, pan scrolls. |
| D-15 | The Claude Design mockups govern UI and UX. UX-01 to UX-04, REV-02, REV-03, REV-04, LAY-02, EXP-01, EXP-02, EXP-03, TXT-03 rewritten to match; UX-06 (Settings) added. The screen-level specification is `docs/design.md`. |
| D-17 | Paper and Bench mockup directions are the light and dark themes (UX-05). |
| D-19 | UX-01: no native menu bar; a title-bar menu button carries the File items. |
| D-21 | UX-02: trial scope defaults to 50 pages; project created next to the source PDF. |
| D-22 | OCR-01: both Tesseract English models bundled, `tessdata_best/eng` default. |
| D-23 | TXT-01: default transcription conventions recorded. |
| D-24 | TXT-03: the default set of auto-apply eligible rules recorded. |

There are no items under review. For any UI question, `docs/design.md` is
authoritative over the prose here.

## 1. Purpose and document conventions

SBWB (Scanned Book Work Bench) is a standalone desktop application for turning scanned historical books into editable Microsoft Word documents. It combines local, open-source document processing with a source-linked review workspace. The product should make a long book manageable without requiring the user to understand OCR engines, launch a service, or inspect hundreds of pages indiscriminately.

This document specifies the product to be built. Its requirements and acceptance targets are not claims about a delivered implementation.

Requirement IDs are stable references. **MUST** means a release requirement; **SHOULD** means a planned capability that can be deferred only with a documented limitation; **MAY** means optional. The optional nature of a feature does not make its safety requirements optional when that feature is provided.

### 1.1 Product outcomes

- Produce readable, editable Word documents while preserving the source's meaning and historical wording.
- Help users find missing, misordered, or questionable content, not merely display OCR confidence.
- Reduce repeated review decisions through prioritization, grouped corrections, and remembered explicit decisions.
- Keep every accepted change explainable, reversible, and linked to the original scan.
- Process substantial books locally, with clear progress, recoverable interruptions, and no mandatory cloud account.

### 1.2 Intended users and material

Primary users are researchers, archivists, editors, and individuals digitizing English-language historical books. They may be comfortable using Word but unfamiliar with OCR terminology.

Seventeenth-century English is generally Early Modern English. The application MUST distinguish this from Old English and Middle English when describing language profiles or model coverage. A modern English model or dictionary MUST NOT be presented as a validated model for every historical period.

Representative material includes degraded print, skewed scans, uneven illumination, bleed-through, long-s and ligatures, multiple columns, narrow marginal notes, running headers, footers, page numbers, catchwords, illustrations, and occasional tables. Difficult handwriting, complex mathematical notation, and non-English passages may require manual transcription.

## 2. Scope and product boundaries

### 2.1 Included

The product covers PDF import, portable projects, image preparation, initial OCR, layout and gap detection, targeted region OCR, contextual text reconstruction, human review, optional AI suggestions, and DOCX export. It includes installers, local model management, recovery, diagnostics, accessibility, and verification of exported content.

Windows 11 x64 is the qualified platform for the first release (D-02). macOS and Linux are intended later release targets: the codebase must not depend on Windows-only APIs, and macOS and Linux builds must keep compiling, but their qualification (NFR-01) is deferred. The core workflow uses open-source components and locally available models. Optional commercial AI services are separate, explicitly enabled integrations (Phase 2, D-05).

The application is implemented in Rust with a Tauri 2 desktop shell and a Svelte 5 + TypeScript + Vite frontend inside the webview (D-01, D-13). No Python or other separately installed language runtime is permitted at run time. Remaining framework choices belong in the software design and must satisfy the same functionality, portability, offline operation, performance, licensing, and safety requirements.

### 2.2 Not promised

- Perfect transcription, automatic historical interpretation, or guaranteed recovery of information absent from a scan.
- Pixel-identical reproduction of the scanned book in Word.
- Silent modernization of spelling, punctuation, grammar, or names.
- Automatic reliable conversion of arbitrary handwriting, equations, music, or complex tables into semantic Word structures.
- Real-time multi-user editing, a hosted web service, or mobile applications.
- Mandatory GPU acceleration, internet access, Microsoft Office, or LibreOffice for local processing and DOCX generation.
- Access to a provider's API through an unsupported consumer-chat login or subscription.

Unrecognized or unsupported content MUST remain visible as a review issue or an explicit export choice; it MUST NOT disappear silently.

## 3. User experience and primary journeys

### UX-01 — Desktop entry and navigation — MUST

The application opens to a Welcome page (design 4.1) with a PDF drop zone and **Choose PDF…**, **Open a .sbwb project…**, and a list of recent books showing each book's scope, stage, and approval progress. There is no native menu bar (D-19); a menu button in the title bar exposes import, open, save, save a copy, export, and close, each with a standard accelerator. Installing and running the application must not require a terminal, a separately installed language runtime, or a manually started server.

An open book uses a single workspace (design section 3): a custom title bar showing the app name, book title, volume, and page scope; a persistent left **pipeline rail** listing Import, OCR, Layout, Text pass, AI proofread, and Export with live state; a centre area that switches between Processing, Review, and Layout modes; a right inspector; and a status bar with the running stage, counters, save state, and project file name. Export is a sheet over the workspace reached from the rail; when it is not ready it shows readiness counters and explains what is missing instead of hiding the action.

### UX-02 — Outcome-oriented processing — MUST

Importing a PDF creates the `.sbwb` project next to the source PDF by default (D-21) and starts OCR immediately on the default scope (the first 50 pages) with the recommended conservative profile, so progress is visible within seconds. The import inspector (design 4.2) shows the verified source, a **Pages to process** choice (First 50 / Range… / All N) that can be changed before or after processing without re-importing, an estimated time labelled as an estimate, the running stage, and a **Start reviewing page 1** action that opens Review while later pages continue.

The Processing view shows a page grid with per-page status, and the pipeline rail carries the primary control for the current state: **Pause all**, **Resume**, or **Retry failed pages**. A job monitor with per-stage cards and a timestamped log is available by expanding the rail; it does not replace the page grid. Completed stages collapse to a single row with their result.

Engine selection, render resolution, deskew, worker count, and text-pass thresholds live on the Settings page (UX-06) under "this book"; region OCR and layout tools live in Layout mode. None of them are mandatory steps for an ordinary book. Static explanations of internal services must not displace the user's content.

### UX-03 — Shared document workspace — MUST

Review displays the scan pane on the left and the complete effective page text (transcript) on the right, with a page filmstrip (Paper) or rail mini-grid (Bench) for navigation and the inspector on the far right (design 4.3). Both panes remain visible while inspecting a word. The window and dividers are resizable; narrow windows stack scan over transcript and turn the inspector into a drawer without losing either view.

Every scan pane uses the same 40px header: ‹ **Page N** of M ›, a page-number field for go-to, **Fit** · zoom percentage with zoom in/out, and a page-label chip showing the printed label and column count when known. Page navigation uses physical PDF page numbers. Zoom is a view setting, not an OCR setting.

Page changes preserve the prior displayed content until the requested replacement is ready, show an honest loading state, and never display a new page label over an old unlabeled image. Stale asynchronous responses must not replace a newer selection. Text and PDF navigation must not visibly flash blank between cached pages.

Preview bitmaps are rendered by PDFium in the backend and cached (D-14). Zoom requests a re-render at the new scale, tiled above a declared bitmap size cap, and keeps the previous scale visible until the new one is ready. Pan is scrolling of the cached bitmap in the webview. Highlight geometry is expressed in PDF page coordinates and transformed per render scale, so the same coordinates serve preview, region OCR, and export mapping.

### UX-04 — Unified review controls — MUST

There is one issue inspector, in the right inspector's **Issue** tab, containing the evidence crop, current reading, score chip with its source, reason, candidates, decisions, and history (design 4.3). OCR and text-pass findings must not require separate dialogs for the same passage. The inspector's other tabs are **Page**, **Text pass**, and **AI**.

The review threshold slider sits in the transcript pane header; issue counts sit in the rail's Review block; the colour legend sits at the foot of the filmstrip. Successful decisions use concise, non-disruptive feedback and optional auto-advance. Technical evidence is one expandable card, not the default focus.

### UX-05 — Accessible, consistent interaction — MUST

All modes share typography, colors, spacing, control styles, and action semantics, defined as tokens in `docs/design.md` with a light theme (Paper, default) and a dark theme (Bench) (D-17). Primary actions, secondary actions, and destructive actions are visually distinct, but color is never the sole indication of status. Keyboard users can navigate pages and issues, adjust zoom and thresholds, inspect evidence, and make decisions. Source highlighting also responds to keyboard focus.

Controls have accessible names, logical focus order, visible focus, and documented shortcuts. Status and progress changes are announced without stealing focus. Core flows must pass a WCAG 2.2 AA-oriented accessibility audit for the desktop interface, including contrast, keyboard access, and text enlargement.

### UX-06 — Settings page — MUST

A single Settings page (design 4.7) groups **this book** settings (Processing, Review, Export defaults) separately from **app** settings (AI providers, Appearance, Shortcuts, Storage & privacy). Each row shows a label, a one-line hint, and its control. Changing a processing setting states its consequence before applying ("Apply · re-run 19 pages") and never touches approved pages without saying so (PIPE-01). Engine and core versions are shown in the navigation footer.

## 4. Projects, source integrity, and persistence

### PRJ-01 — Immutable source and import validation — MUST

Import validates PDF readability, page count, available disk space, and encryption requirements. Password-protected inputs use an explicit password prompt; passwords are not retained by default. Unsupported, malformed, or excessively large inputs produce actionable errors.

The project stores a verified copy of the original PDF with a cryptographic checksum. Processing never modifies that copy. Cached renders and processed images are derived artifacts, not replacements for the source. Source identity is checked before operations that depend on it; a mismatch blocks those operations until resolved.

### PRJ-02 — Portable project — MUST

A self-contained `.sbwb` project contains its source, committed transcript, layout, settings, provenance, decisions, processing state, and export history. It can be saved as a consistent portable copy through the UI. It must not depend on absolute source paths, API credentials, or a model remaining installed to view and export already committed text.

Project schema and application compatibility are explicit. Opening an unsupported format must not damage it. Any supported format transformation requires a recoverable backup and verified result. Only one writer may open a project at a time; another instance can offer read-only access.

### PRJ-03 — Page scope and incremental work — MUST

Processing scope is recorded separately from the source page count. A quick trial retains the whole original PDF. Users can extend a trial to more pages or the full book without reimporting, discarding corrections, or repeating valid completed work. Unprocessed, excluded, failed, and completed pages are distinguishable in navigation, review summaries, and export settings.

### PRJ-04 — Saved changes, drafts, and recovery — MUST

Edits commit transactionally and show **Saving**, **Saved**, or **Save failed** accurately. A draft remains visible until acknowledged as durable or explicitly discarded. Navigation that would lose a draft is guarded; save failure never dismisses it.

Closing with unsaved work offers **Save and close**, **Discard unsaved changes and close**, and **Cancel**. Discard removes only uncommitted drafts, not saved corrections. Closing during a job offers a safe checkpoint-and-close path. Failed saving or checkpointing keeps the application open with a clear explanation. Crash recovery distinguishes committed state, recoverable drafts, and incomplete tasks.

### PRJ-05 — History and privacy — MUST

Text, layout, decisions, approvals, and grouped edits have durable history and undo where preconditions allow it. Undo must not overwrite a later unrelated edit. Project diagnostics and application settings are separated from document content. Clearing recent projects does not delete projects; deleting project data requires an explicit, scoped user action.

### PRJ-06 — Multiple-book queue — SHOULD

Users can queue several PDFs with an explicitly chosen processing profile and destination, creating one independent project per book. The queue shows per-book status and allows pending books to be reordered or removed without deleting their projects. A failed book does not prevent unrelated queued books from proceeding. Resource limits apply across the queue, and queue resumption after application restart requires an explicit action. Review and export decisions remain book-specific.

## 5. Processing and recognition

### PIPE-01 — Automatic multi-pass plan — MUST

The standard local plan performs preparation, initial OCR, layout analysis, targeted region OCR and coverage checking, text reconstruction, and review indexing. Initial OCR informs layout, but independent image analysis must also detect text-like areas missed by OCR. Refinement loops are bounded and report unresolved gaps rather than running indefinitely.

The plan records page scope, algorithms, model identities, settings, and input revisions. Changing a setting invalidates only affected dependent results. Reruns retain raw evidence, manual edits, rejected proposals, and explicit approvals in history; they never silently overwrite human decisions.

### PIPE-02 — Progress and control — MUST

The UI reports current stage, completed/total pages or regions where known, elapsed time, activity, failures, and saved progress. Estimated remaining time is labeled as an estimate and omitted when unreliable. An operation with unknown work reports activity instead of an invented percentage.

Pause and cancellation are acknowledged promptly and take effect at documented page, region, or request boundaries. Completed work survives both. Resume continues a saved plan; retry can target failed units. A failed page does not erase successful pages. Opening a project never silently starts heavy processing or a network request.

### IMG-01 — Preparation and image evidence — MUST

Preparation supports orientation correction, deskew, conservative contrast/background correction, and optional binarization or noise reduction. Originals and useful variants remain distinguishable. Aggressive cleanup that can erase faint text is optional and reviewable.

Preview renders and recognition renders are separate artifacts even though both come from PDFium (D-14); OCR never consumes a preview bitmap. Region OCR renders from the source PDF at a nominal **300 DPI**, not from a low-resolution UI preview. Input and output geometry, actual pixel dimensions, transformations, and quality warnings are recorded. Nominal rendering resolution must not be represented as recovery of detail absent from the embedded scan.

### OCR-01 — Local engines and model packs — MUST

Selectable local options include Tesseract English standard (`tessdata/eng`) and **`tessdata_best/eng`**; both are bundled and `tessdata_best/eng` is the default (D-22). A clearly named optional second engine MAY be offered for disagreement evidence; if provided it must be a locally runnable engine with no separate language runtime (D-03). The UI identifies whether a configuration supports page detection, line recognition, or both. Engine/model downloads, redistribution licenses, checksums, supported platforms, and capability tests are release gates.

An offline installation option includes the standard English workflow. Additional model packs can be installed explicitly from a verified catalog or approved local package. Missing optional models must not prevent opening a project. A model's native line score must not be invented as a word score, and scores from different engines must not be averaged without a validated method.

### OCR-02 — Targeted refinement and missing text — MUST

Low-quality text, disagreements, small print, marginalia, headers, footers, and independently detected uncovered text regions can trigger targeted OCR. A manually drawn text region can request recognition even if initial OCR found no lines there.

Marginalia and other small-text refinement support a reproducible **fixed 3× enlargement** of a crop rendered at 300 DPI. Both dimensions are enlarged before the image is submitted to the selected recognizer. Exact input images, crop padding, interpolation, scale, and model preprocessing are inspectable. The 3× diagnostic profile is deterministic; a resource limit produces an explicit failure or an offered alternative, not silent scale reduction.

Alternative candidates are retained. Engine disagreement and text-coverage gaps become review evidence, including where no OCR confidence exists. Adding candidates must not replace human-approved or manually edited text automatically.

### LAY-01 — Layout, gaps, and reading order — MUST

Layout distinguishes body, heading, header, footer, marginalia, footnote, page number, catchword, illustration, table, and uncertain regions. It detects persistent whitespace gutters and text boundaries to separate columns and side notes, even when an OCR line spans a gap.

The primary layout source is independent image analysis of the prepared page (projection profiles, whitespace rectangles, connected components, baseline clustering) that does not depend on OCR output (D-11). Tesseract's own page segmentation is also run and recorded as additional evidence; disagreement between the two sources is a review signal, and neither source alone is treated as proof of a blank region.

Reading order follows the content structure rather than simply sorting all text by vertical coordinate. Full-width headings, column changes, inset notes, sparse pages, and decorative gaps receive explicit handling or review warnings. Text from adjacent columns must not be joined solely because it occupies the same horizontal band.

### LAY-02 — Visual correction — MUST

Layout mode (design 4.4) replaces the transcript with a reading-order list and the inspector with a region inspector while the scan stays in place. Tools are **Select (V)**, **Draw (R)**, **Split (X)**, **Merge (M)**; regions show numbered type tags and handles when selected. The reading-order list is drag-reorderable and shows each region's export placement. The region inspector offers type chips (body, header, marginalia, footnote, page no., ignore, catchword, illustration, table, uncertain), **In Word** structure (ordinary text, heading level, table cell), bounds, split/merge actions, and **Revert** / **Save layout**. Rectangle selection and additive selection allow batch classification of explicitly selected items. Selection and an affected-item preview precede changes that clear incompatible structure assignments, and the inspector states that changing a body region re-runs the text pass for that page and clears its approval.

The source stays visible and zoomable throughout. Every operation is reversible and preserves source links. A blank new region can be labeled for OCR, illustration inclusion, or intentional exclusion. Layout edits invalidate only affected derived results and approvals, with the impact shown before a costly rerun.

### LAY-03 — Coverage accountability — MUST

For each processed page, the product records recognized text regions, probable uncovered text, non-text regions, and explicit exclusions. It flags clipping, suspicious empty areas, overlapping ownership, and ambiguous reading order. Detection uncertainty is visible; "no OCR text" must not be treated as proof that a page or region is blank.

## 6. Text reconstruction and provenance

### TXT-01 — Historical fidelity — MUST

The default policy preserves historical spelling, names, dialect, and meaning. The user can protect project vocabulary and record preferred readings. Unknown words are not automatically mistakes; dictionary frequency alone is insufficient evidence for automatic correction. Long-s, ligatures, punctuation, names, and genuine compounds require conservative treatment.

Source glyphs, Unicode transcription choices, and optional editorial normalization are distinct. The default transcription conventions (D-23) are: long-s as "s", fi/fl/ff/ffi/ffl ligatures as separate letters, æ and œ kept, spelling, capitalisation, and punctuation preserved. They are project settings and every affected span records which convention produced it. Any normalization that changes wording is explicit, source-linked, and reversible. A historical language label must not imply accuracy that has not been evaluated for that profile.

### TXT-02 — Paragraphs and word reconstruction — MUST

The text pass uses saved text and layout context to propose spelling corrections and resolve line/page hyphenation. It joins physical scan lines into logical paragraphs using indentation, spacing, reading order, and explicit user structure, not a newline after every sentence.

Cross-line and cross-page word joins preserve punctuation and source anchors on every participating page. Joins never cross a column, heading, note, exclusion, unprocessed page, or uncertain boundary without explicit evidence or review. Genuine compound hyphens and user-entered paragraph breaks are protected. All affected fragments change or undo atomically.

### TXT-03 — Automatic application policy — MUST

The offline text-pass auto-apply threshold is user-defined, defaults to **90/100**, and is separate from the review threshold. A score at or above the threshold is necessary but not sufficient: automatic application also requires an eligible rule (by default only hyphen joins across line and page breaks and lexicon-supported single-character OCR confusions, D-24), valid source anchors, unchanged revisions, and absence of protected human decisions or conflicting evidence.

The inspector's **Text pass** tab (design 4.6) exposes the auto-apply slider, applied / suggested / undone counters, **Re-run pass**, and **Review N →**. Modernization, uncertain proper-name changes, unsupported grammar rewrites, and AI proposals are not automatically applied. Dictionary-only suggestions are not eligible merely because a user lowers the threshold. The UI labels heuristic scores as scores rather than calibrated probabilities. Automatically applied changes remain distinguishable from human approval and can be inspected and undone.

### PROV-01 — Complete source mapping — MUST

Every effective text span maps to one or more original source regions, or is explicitly labeled as user-added text with no exact source. Evidence records include source hash/page, bounding geometry, coordinate transforms, raw OCR, candidate text, engine/model and algorithm versions, available native scores, processing settings, proposal reasons, and decision history.

Edits never inherit an original word's confidence as if it described the corrected word. When exact word alignment is unavailable, the UI highlights the enclosing line or region and states the limitation. Exported text must be traceable through the same mapping, including cross-page joins and native Word headers/footers.

## 7. Review that minimizes human effort

### REV-01 — Book-wide issue inbox — MUST

Review begins with a prioritized, book-wide inbox of unresolved issues, not an obligation to inspect every low-scoring token. Issue types include probable missing text, column/order ambiguity, clipping, conflicting readings, risky substitutions, questionable word joins, and explicit user flags. Each item explains why it matters and shows the relevant scan and surrounding text.

Priority considers likely impact and evidence, not confidence alone. Structural and missing-text issues remain discoverable without numerical scores. The UI reports total unresolved, currently matching, deferred, resolved, stale, and unprocessed coverage separately; a partially built index must never report a complete book as having zero issues.

### REV-02 — Thresholds and view filters — MUST

The review threshold slider in the transcript header defaults to **70/100** (per the mockups, D-15), ranges from 0 to 100, and selects scored unresolved text issues **strictly below** its value. Equal/higher scores are unmarked, not approved. Unscored structural issues remain in the inbox unless their category is explicitly filtered. AI proposal scores are labeled separately and do not masquerade as native OCR confidence or correctness probabilities.

Separate options hide completed, automatically applied, and human-approved markers. Hiding a marker never removes text or changes a decision. A full-page reading view remains available at every threshold. The threshold, sorting, and filters do not trigger OCR or automatically approve anything.

### REV-03 — One decision workspace — MUST

Hovering or focusing a marked span highlights its source word on the scan; selecting it loads the Issue tab. The tab shows the evidence crop with surrounding text, the current reading, a score chip labelled with its source ("64% · OCR"), a reason line, numbered candidates (including the raw OCR reading), and history, with **Accept (A)**, **Edit (E)**, **Skip (S)**, and **Later (L)**. Skip keeps the original and resolves that exact proposal; Later defers it.

Selecting a candidate is not a commit. Skip does not certify unrelated issues. Moving away without a decision changes nothing except explicitly saved edits. Manual edits do not silently approve the page. Cross-page evidence can be inspected without dismissing the active issue.

### REV-04 — Efficient unresolved navigation — MUST

**Next issue (J)** and **Previous issue (K)** visit only unresolved, non-deferred items matching the current scope and filters, in an explicit priority or reading-order mode. They traverse the book, deduplicate linked cross-page items, and wrap at most once before reporting that no matches remain.

Resolved and threshold-hidden items are skipped without being reclassified. Optional automatic advance moves to the next issue only after a successful durable decision. Keyboard shortcuts, selection, and source highlighting use the same behavior as buttons. Loading/index failures are reported instead of silently skipped.

### REV-05 — Safe grouped corrections — MUST

After a user accepts a spelling correction, the application can offer matching occurrences across the selected scope. A preview shows the exact original/replacement pair, occurrence count, page/context for each match, conflicts, protected text, and selectable exclusions. Only explicitly selected eligible matches are changed.

Matching is whole-token or precisely anchored phrase matching, not unrestricted substring replacement. Context-dependent names, capitalization, punctuation, and differing readings are not silently grouped. The operation is atomic, creates a named history entry, and supports guarded undo. It creates no permanent global replacement rule and does not approve affected pages.

### REV-06 — Decisions, deferral, and approval — MUST

Decisions are anchored to source and text revisions. Unchanged rejected or approved issues do not reappear after a rerun. New conflicting evidence is distinguishable from an old resolved issue. Deferred issues remain unresolved, have a dedicated view, and are included in export warnings.

Users may approve a reviewed region or page without editing each token, using **Approve page (Cmd/Ctrl+Enter)** in the inspector footer, which shows the page's unresolved count beside it. Approval records the exact covered revision and outstanding-issue acknowledgements. Later relevant text/layout/evidence changes require fresh approval. The UI distinguishes **no matching issues**, **no unresolved issues**, and **human approved**.

### REV-07 — Removed (D-10)

The review-effectiveness usability study and its 25% median-time target were dropped on 2026-09-12. The ID is reserved so older references stay unambiguous. Local review-session instrumentation may still be built as a MAY item but is not a release gate.

## 8. Optional AI reconstruction — Phase 2 (D-05)

Section 8 is not part of the first release. Its requirements remain in force
for whenever the feature ships, and the first release must design the review
inbox, history, and provenance records so provider-labelled proposals can be
added without a schema change. In the first release the rail's **AI
proofread** row and the inspector's **AI** tab render in an "off" state
whose "enable" link explains that the feature arrives in a later version
(design 4.6).

### AI-01 — Provider connections — MUST when enabled

An optional **AI reconstruction** section offers OpenAI, Anthropic, and Google provider adapters and an account-accessible model selector. Connection, model discovery, and request compatibility errors are separate and actionable. A discovery failure must not falsely report that the key could not be saved; the UI can retain the connection and offer refresh or an explicitly validated supported model ID.

API keys are entered through a secure desktop-owned path, are session-only by default, and may be remembered only using available OS-backed secure storage. They are never stored in projects, exports, ordinary logs, or browser persistence. Unsupported account login must not be offered. The UI explains that consumer subscriptions and developer API entitlement are separate concerns to verify with the provider.

### AI-02 — Explicit, bounded disclosure — MUST when enabled

Each run requires explicit consent showing provider, model, selected pages, exact kinds of text/context to be sent, and any available usage/cost estimate. Default payloads contain selected saved text and minimal source identifiers, not PDFs, scan images, local paths, or project metadata. Context outside the selection must be identified and included in consent or omitted.

No AI requests run automatically on import, open, resume of local processing, or model selection. Users can test one page, set a request/token budget, cancel, and inspect run history. Unknown costs are labeled unknown. Timeouts and ambiguous failures must not lead to unbounded retries or repeated billable requests without explicit user action.

### AI-03 — Suggestions, not authoritative rewrites — MUST when enabled

AI returns anchored spelling/grammar suggestions with before/after text and reasons. Responses are validated against the unchanged requested text. Ambiguous, ungrounded, out-of-scope, or stale replacements are rejected or flagged without application. Document content is untrusted data, never an instruction to the application or provider tool layer.

AI suggestions enter the same review inbox and history as local suggestions, clearly labeled by provider/model. They are **never automatically applied**, irrespective of any score or offline threshold. Local processing, manual review, and export remain usable with no AI connection.

## 9. Word export and content accountability

### EXP-01 — Visible, editable output — MUST

The Export sheet (design 4.5) opens over the workspace with readiness counters (pages approved, unresolved below the threshold, AI suggestions pending), the scope, the destination, and two choices. **Working copy** exports the current text as-is with unresolved words highlighted and one Word comment per flag, and is always available. **Clean copy** contains no annotations and requires every page in the selected scope to be currently human-approved with no unacknowledged unresolved or deferred issues; the sheet shows how many pages are left, and warnings alone cannot bypass this gate.

DOCX contains real editable text, paragraphs, named styles, reviewed heading levels, and supported simple tables. Highlights and comments appear only in the working copy; the clean copy carries no review annotations of any kind. Word or LibreOffice is not a runtime prerequisite for creation.

### EXP-02 — Paragraphs and original page boundaries — MUST

Word output uses logical paragraphs, not one paragraph per scan line. **Mirror the scan** (the default page-structure choice; the alternative is **Continuous text**) inserts an actual Word page break after the content assigned to each included source page; when the native headers/footers option (EXP-03) is selected, the break is a next-page section break instead. It does not insert literal text such as `[Source page 50]`.

For a word reconstructed across pages, the complete word belongs to the earlier page and the boundary follows it; consumed continuation characters are not duplicated. This deliberate policy is explained and recorded. Continuous text mode is optional and must state how page-specific headers, footers, and marginalia will be handled. The sheet's Marginal notes, Footnotes, and Source page numbers checkboxes control inclusion under either choice. Word may repaginate or overflow a source-page section; source-boundary preservation is not a promise of identical physical pagination.

### EXP-03 — Headers, footers, and other content — MUST

Page furniture placement is an export setting with two policies (D-12):

- **Styled paragraphs (default).** Reviewed header regions are emitted as body paragraphs in a named **Running head** style at the top of each source page's content, printed page numbers in a **Printed page number** style, included marginalia in a **Side note** style immediately before the body paragraph they annotate, and reviewed footer regions in a **Footer note** style after the page's body. The styles are visually distinct, editable, and searchable, and the text survives Word repagination.
- **Native Word headers and footers (option).** Reviewed header regions go into native Word page headers, and reviewed footer regions and included marginalia go into native Word page footers, with one next-page section per source page. Source-page sections preserve page-specific furniture without unintentionally inheriting another page's text. The option explains that every page ends in a section break.

Under either policy, order within a page's furniture is deterministic and previewable, and every emitted furniture span keeps its source mapping (PROV-01). The reading-order list in Layout mode shows each region's resulting placement ("→ Word header", "→ footer note", "archive only") so the user can see the policy's effect per page before exporting.

The export UI also provides explicit inclusion/placement policies for page numbers, footnotes, catchwords, illustrations, uncertain regions, and tables. Footnotes without reliable callout links may be retained as labeled notes rather than fabricated semantic footnotes. Illustrations can be included as images; unsupported tables can be preserved as images or explicitly selected text with a warning. Exclusions are listed before export and in the report.

### EXP-04 — Snapshot, progress, and validation — MUST

An export uses an immutable snapshot of selected text, layout, approvals, source mappings, and settings. Progress exposes real phases and completed work, with useful activity during packaging. Cancellation and failures leave committed project data and prior exports intact and never label a partial file as successful.

Validation checks OOXML structure, selected content coverage, duplicates/omissions, page-boundary policy, header/footer linkage, and text parity against the snapshot. Validation does not claim a particular Word pagination unless an actual rendering check has been performed. Output and a concise report are published atomically, with checksum and settings recorded in history.

### EXP-05 — Archival outputs — SHOULD

Offer a companion archive containing a machine-readable transcript/source map, reviewed layout interchange such as PAGE XML, immutable export manifest, and validation report. Preserve raw and effective text distinctions. Every emitted body/header/footer/note span and every deliberately excluded region must be accounted for. User text with no scan anchor remains explicitly identified.

### EXP-06 — Formatting and metadata — MUST

Users can choose a reusable export preset and adjust page size, margins, body font/size, paragraph spacing, and supported heading/note styles. Drop-cap settings and the number of lines a drop cap spans are grouped together. Unsupported or unavailable fonts produce an explicit substitution warning rather than a claim of exact source typography.

An advanced **PDF metadata** area displays source metadata separately from editable project and Word document properties such as title and author. Editing output metadata does not modify the original PDF. Formatting and metadata choices are saved in the export snapshot and can be reused without altering transcribed wording or review decisions.

## 10. Nonfunctional requirements and release targets

The following are proposed acceptance targets. Each benchmark report must identify build, OS, CPU, memory, storage, model versions, selected profile, input hashes, warm/cold cache state, and failures. Targets must not be advertised as measured results before validation.

| ID | Area | Requirement or measurable target |
| --- | --- | --- |
| NFR-01 | Platform | First release: install/run/uninstall qualification on Windows 11 x64 (D-02). macOS (Apple Silicon) and Linux (x64 LTS) builds must compile and are qualified in a later release; until then they are not advertised as supported. Publish exact tested OS/architecture combinations. |
| NFR-02 | Reference workload | Reference corpus is Hough, *The History of Christianity in India* vol. 1 (1839), see `fixtures/README.md` (D-06): 10-page trial = pages 1-10, 100-page mixed-layout book = pages 1-110, 500-page book = full 529-page scan. Add a 250,000-word export fixture with notes, two-column pages, and at least 10,000 review records. Use an 8-core CPU, 16 GB RAM, SSD Windows machine as the primary reference, with exact hardware recorded. |
| NFR-03 | Interaction | On the reference machine, welcome-ready p95 at most 5 seconds; indexed project summary p95 at most 3 seconds; cached page navigation p95 at most 250 ms; uncached preview p95 at most 2 seconds; indexed next-issue query p95 at most 300 ms. Report end-to-end loading separately from query time. |
| NFR-04 | Resource bounds | Standard CPU processing on the reference workload targets at most 6 GB aggregate resident memory for the application and workers. Queues, render caches, model instances, and crop sizes have explicit caps. Oversized pages produce an actionable limit warning; they do not crash the application or silently lose text. |
| NFR-05 | Export | The reference 500-page, 250,000-word text-oriented fixture targets ordinary DOCX creation and validation within 60 seconds, with annotation and illustration-heavy profiles measured separately. Time and memory should grow approximately linearly with emitted content and selected evidence, not with content multiplied by total history. |
| NFR-06 | Control responsiveness | Pause/cancel acknowledgement p95 at most 500 ms; visible progress/activity at least every 2 seconds while active. Local worker units have a declared timeout, initially 120 seconds, and safe cancellation escalation after a 10-second grace period. Completed commits survive interruption. No forced desktop termination is part of normal operation. |
| NFR-07 | Throughput | CPU worker count and engine thread count are bounded to avoid oversubscription. On the fixed CPU OCR fixture, four workers should achieve at least 2× single-worker throughput without violating memory limits; a failed target requires profiling and a documented release decision. GPU acceleration is optional and must demonstrate benefit on the actual workload. |
| NFR-08 | Recognition quality | Report CER/WER, region coverage, missed-line rate, column/order errors, and correction precision by scan condition and region class. Establish and freeze numerical OCR/coverage release gates after the annotated-corpus baseline milestone, before release qualification; until then, no universal accuracy claim is permitted. |
| NFR-09 | Automatic corrections | On an independently adjudicated held-out set, eligible offline auto-corrections target at least 99% precision, with sample count and uncertainty interval reported. A profile that fails this gate ships in suggestion-only mode. Historical spellings and proper-name damage are tracked separately. |
| NFR-10 | Durability | Crash, forced worker exit, disk-full, and restart tests must demonstrate no lost acknowledged edits, no half-applied cross-page/group transactions, and no published partial exports. Crash-recovery drafts are distinguished from acknowledged saved data. |
| NFR-11 | Privacy/security | Core processing works offline; there is no document telemetry by default. Diagnostics are opt-in, previewable, and redacted. Untrusted PDFs, images, project archives, and model packages are parsed with resource/path restrictions and isolated worker boundaries. |
| NFR-12 | Distribution | Deliver self-contained packages, checksummed artifacts, dependency/model inventories, required license notices, and a tested offline installation path. Public Windows installers require signing; macOS notarization and Linux verification information apply when those platforms are qualified. |
| NFR-13 | Maintainability | Typed, versioned boundaries separate UI, workflow, recognition, review, persistence, and export. Automated unit, integration, recovery, accessibility, packaging, and document-validation tests gate releases. No presentation component owns processing or persistence rules. |

## 11. Acceptance scenarios

Each scenario is executable as a documented manual test, automated test, or both. Test evidence names the relevant requirement IDs and fixture revisions.

| Scenario | Required result | Requirements |
| --- | --- | --- |
| A-01 Offline first use | On a clean Windows machine with networking disabled and no development tools or Office suite installed, import a five-page trial, process, review, and export editable DOCX through the GUI. | UX-01, UX-02, OCR-01, EXP-01, NFR-01, NFR-12 |
| A-02 Extend a trial | Increase a five-page trial to a larger range. Previously corrected text and valid artifacts remain; excluded and unprocessed pages remain distinguishable. | PRJ-03, PIPE-01 |
| A-03 Two-column pages | On ground-truthed pages with visible gutters, separate body columns from marginalia, preserve reading order, and flag ambiguous gaps rather than merging across them. | LAY-01, LAY-03 |
| A-04 Missing marginal line | A scan line omitted by initial OCR is independently detected or manually region-selected, submitted as a recorded 300-DPI crop enlarged exactly 3×, and reviewed with accurate source highlighting. | IMG-01, OCR-02, PROV-01 |
| A-05 Historical wording | Protected historical forms and names survive local and AI checks. A genuine ambiguous hyphen remains reviewable rather than being silently normalized. | TXT-01, TXT-02, TXT-03, AI-03 |
| A-06 Cross-page correction | Accept and undo a joined word across adjacent pages. Both fragments and export mappings change atomically; a later conflicting edit blocks unsafe undo. | TXT-02, PRJ-05, EXP-02 |
| A-07 Efficient review | Navigate across the book with a threshold, deferred item, resolved item, unscored gap, and linked proposal. Only eligible items are visited; counts explain every excluded category. | REV-01, REV-02, REV-04, REV-06 |
| A-08 Group safety | Preview 20 matching corrections, exclude 3, and encounter 1 stale match. No partial commit occurs until the selection is refreshed; one guarded operation applies and can undo the approved set. | REV-05, PRJ-05 |
| A-09 Rerun protection | Change a model and rerun affected pages. Raw evidence survives, manual text is unchanged, old exact decisions stay resolved, and relevant approval changes are explicit. | PIPE-01, OCR-02, REV-06 |
| A-10 Close and recovery | Exercise save, discard, cancel, save failure, worker crash, and interrupted export. Acknowledged edits survive, drafts are not silently discarded, and prior exports stay valid. | PRJ-04, PIPE-02, EXP-04, NFR-10 |
| A-11 Optional cloud use (Phase 2) | Mock connection/discovery failures, consent, budget exhaustion, timeout, and invalid AI anchors. No unauthorized content is sent, no key enters a project/log, and no proposal auto-applies. | AI-01, AI-02, AI-03 |
| A-12 Word structure | Export different headers/footers and marginalia on consecutive pages under both placement policies. Verify the named furniture styles and page breaks in the default policy, native stories and section boundaries in the optional policy, no source-marker strings, no default comments, no duplicate/missing joined word, selected formatting/metadata, and explicit excluded-content accounting. | EXP-01, EXP-02, EXP-03, EXP-04, EXP-05, EXP-06 |
| A-13 Smooth and accessible review | On a wide display and at enlarged text size, resize the split, jump/zoom pages, inspect with keyboard, and save a decision without losing focus, showing mismatched pages, or blank flicker. | UX-03, UX-04, UX-05, NFR-03 |
| A-14 Large book | Run the reference corpus with recorded resource use and progress, resume interrupted work, and meet qualified export/interaction targets without loading the whole book's images or history. | NFR-02 through NFR-10 |

## 12. Release readiness and unresolved specification decisions

Release readiness requires all MUST requirements, executed acceptance scenarios, platform qualification, and published limitations. Optional AI can be disabled in a package, but enabling it requires all AI safeguards. SHOULD items not delivered must appear in release notes and in affected UI workflows.

Before implementation commitments are finalized, record these decisions with owners and test evidence:

1. Exact supported OS versions/architectures and verified dependency/model redistribution terms.
2. Annotated historical corpus, region-level ground truth, and OCR/coverage quality gates.
3. Validated model configurations and language-profile coverage, including which Tesseract model packs are packaged and whether an optional second engine ships.
4. Typography defaults and tested Word rendering combinations, including page-furniture overflow handling.
5. Secure credential-store behavior on each supported desktop and permitted authentication mechanisms for each optional provider (Phase 2).

These are explicit qualification tasks, not permission to relax content preservation, consent, or durability requirements.
