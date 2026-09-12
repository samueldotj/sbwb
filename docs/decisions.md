# SBWB decision log

Decisions that are not derivable from the requirements document or the code.
Each entry names the requirement it touches.

## Settled

| ID | Decision | Touches | Date |
| --- | --- | --- | --- |
| D-01 | Rust core with a Tauri 2 desktop shell. | UX-01, UX-05, NFR-13 | 2026-09-12 |
| D-02 | Windows 11 x64 is the only qualified platform for the first release. Code must not use Windows-only APIs; macOS and Linux builds stay compiling. | NFR-01, NFR-12 | 2026-09-12 |
| D-03 | PaddleOCR is removed from OCR-01. Tesseract 5 with `eng` and `tessdata_best/eng` is the primary engine. A second engine (pure-Rust `ocrs`) is optional and used only for disagreement evidence. | OCR-01, OCR-02 | 2026-09-12 |
| D-04 | Application license is MIT. GPL dependencies are acceptable only when no adequate alternative exists. PDF rendering uses PDFium (BSD), so no GPL dependency is currently needed. | NFR-12 | 2026-09-12 |
| D-05 | Optional AI reconstruction (section 8) is Phase 2. The review inbox and history are designed to accept provider-labelled proposals so Phase 2 does not change the schema. | AI-01..03 | 2026-09-12 |
| D-06 | Reference corpus is Hough 1839 vol. 1, trial scope pages 1-110. See `fixtures/README.md`. The language profile is 19th-century English; Early Modern English coverage is evaluated later with a separate fixture. | NFR-02, TXT-01 | 2026-09-12 |
| D-07 | Offline lexicons: Webster 1913 + SCOWL/LibreOffice Hunspell en_GB, suggestion-only until NFR-09 precision is measured. | TXT-02, TXT-03, NFR-09 | 2026-09-12 |
| D-08 | The pasted PRD is an AI summary of a manually written canonical document. On conflict the manual document wins and the conflict is recorded here. | all | 2026-09-12 |
| D-09 | Project file `.sbwb` is a single SQLite database (source PDF stored as a blob, transcript, layout, decisions, history). Page renders live in a rebuildable cache directory next to it, never inside it. | PRJ-02, NFR-03 | 2026-09-12 |
| D-10 | REV-07 (review-effectiveness study) is dropped. The ID stays reserved; local review-session instrumentation may still be built but is not a release gate. | REV-07 | 2026-09-12 |
| D-11 | Layout analysis uses classical computer vision (projection profiles, whitespace rectangles, connected components, baseline clustering) as the primary, independent source. Tesseract's own page segmentation is run and recorded as extra evidence, never as the sole source. ONNX layout models are not in scope. | LAY-01, LAY-03, PIPE-01 | 2026-09-12 |
| D-12 | Page furniture in DOCX defaults to styled body paragraphs (Running head, Printed page number, Side note, Footer note styles). Native Word headers/footers with one section per source page is an explicit export option. | EXP-02, EXP-03, A-12 | 2026-09-12 |
| D-13 | Frontend inside Tauri is Svelte 5 + TypeScript + Vite, with Bits UI for accessible primitives. Originally React on 2026-09-12; switched to Svelte the same day at the user's request for a smaller runtime and less boilerplate, accepting thinner drag-and-drop and slider libraries, which get explicit keyboard tests. | UX-05, NFR-13 | 2026-09-12 |
| D-14 | Previews are rendered by PDFium in the Rust backend and cached as WebP. Zoom re-renders at the requested scale (tiled above a size cap); pan is webview scrolling of the cached bitmap. PDFium is a rasterizer, so zoom and pan are implemented by the preview component, not the library. | UX-03, IMG-01, NFR-03 | 2026-09-12 |
| D-15 | The Claude Design mockups ("Book Scanner Redesign", ten artboards) govern UI and UX and take priority over the requirements text. Requirements were amended to match on 2026-09-12; `docs/design.md` is the derived specification. Exception: mockup helper copy that contradicts a settled export or processing decision (D-12 header placement) is adjusted to the decision rather than the other way round, because the copy is one sentence and the decision was made with the options in view. | UX-01..05, REV-02, REV-03, EXP-01..03, LAY-02 | 2026-09-12 |
| D-16 | Mockup export files (`design/sbwb-redesign/`) are not committed. They stay on disk for reference and are git-ignored; only `design/README.md` and `docs/design.md` are versioned. | repo | 2026-09-12 |
| D-17 | The two mockup directions become the app's two themes: "Paper" is the light theme and default, "Bench" is the dark theme. Both share one layout skeleton, so the theme is a token swap, not two layouts. | UX-05, docs/design.md | 2026-09-12 |
| D-18 | Bundled UI fonts: Source Sans 3, Newsreader, Instrument Sans, JetBrains Mono (all SIL OFL), shipped with the app; no Google Fonts requests at run time. | NFR-11, NFR-12 | 2026-09-12 |
| D-19 | No native menu bar. The custom title bar carries a menu button with the File items (import, open, save, save a copy, export, close) and the standard accelerators. | UX-01 | 2026-09-12 |
| D-20 | The custom title bar loses Windows snap-layout hover on the maximize button. Accepted for the first release; revisit in M9. | UX-01, NFR-01 | 2026-09-12 |
| D-21 | Import defaults: trial scope is the first 50 pages; the `.sbwb` project is created next to the source PDF, with a Change link in the import inspector. | UX-02, PRJ-03 | 2026-09-12 |
| D-22 | Both `eng` (fast) and `tessdata_best/eng` are bundled in the installer; `tessdata_best/eng` is the default recognizer. | OCR-01, NFR-12 | 2026-09-12 |
| D-23 | Default transcription conventions: long-s is transcribed as "s"; fi, fl, ff, ffi, ffl ligatures become separate letters; æ and œ are kept; original spelling, capitalisation, and punctuation are preserved. Recorded per span as transcription choices, not corrections; changeable per project. | TXT-01 | 2026-09-12 |
| D-24 | Auto-apply eligible rules at or above the threshold: (a) hyphen joins across line and page breaks; (b) single-character OCR confusions (rn/m, l/1, I/l, o/0, c/e, ſ/f) where the result is in the lexicon or project vocabulary and the original is not. All other kinds remain suggestions. | TXT-03, NFR-09 | 2026-09-12 |
| D-25 | Tauri bundle identifier is `io.github.samueldotj.sbwb`. It names the AppData folder, the WebView2 profile folder, and the installer's upgrade identity; it is not shown in the UI. Fixed from the first public build onward. | NFR-12 | 2026-09-12 |
| D-26 | Ground truth for M4.8 and M5.9: Claude drafts region, reading-order, and text annotations for about 20 fixture pages; the user spot-checks 5; the remainder is reported as unadjudicated in quality metrics. | NFR-08, NFR-09 | 2026-09-12 |
| D-27 | The text pass is pure Rust and runs on a dedicated thread inside the scheduler (its own SQLite connection; the writer lock is per process), not in a worker process. One lexicon is loaded per run; project vocabulary and protected words are folded in at start. Dictionary suggestions are budgeted per page (40) because each is a Hunspell edit search. | TXT-01, TXT-03, NFR-03 | 2026-09-12 |
| D-28 | Auto-apply guards for OCR confusions: exactly one single-character candidate may be a dictionary word (ambiguity drops the score to 78) and the word needs four or more letters (shorter drops to 84), so both stay suggestions under the default threshold of 90. Spans produced by a hyphen join are never spell-checked again as fragments; roman numerals are skipped. | TXT-03, D-24 | 2026-09-12 |
| D-29 | Approval is not cleared by later changes; the page keeps its approved revisions and reports `outdated` until approved again, so the record of what was approved survives (REV-06). Reruns of approved pages stay blocked (PIPE-01) until the approval is removed. | REV-06, PIPE-01 | 2026-09-12 |
| D-30 | `docx-rs` builds the document and registers per-section headers, but drops per-section footers and exposes no per-section page geometry or core properties; the export post-processes the package (footer parts, relationships, content types, `pgSz`/`pgMar`/`type`, `core.xml`) instead of switching writers. Drop caps are emulated as a framed first-letter paragraph, which is Word's own construction. | EXP-02, EXP-03, EXP-06 | 2026-09-12 |

## Open

None. Options considered for D-10 through D-14 are kept below for the record.

## Record of options considered

### O-01 REV-07 review-effectiveness study (resolved: D-10, dropped)

REV-07 as written requires a usability study: a fixed page set with ground
truth, participants reviewing the same OCR output under two conditions
(unprioritized page-by-page vs. the issue inbox), and an independent person
adjudicating residual serious errors. The 25% median time reduction is the
pass mark. That is a research protocol, not something a solo developer can
satisfy at release.

Options:

1. Keep the metric instrumentation (local, opt-in review-session timing:
   time per decision, clicks per corrected issue, batch corrections undone)
   but downgrade the 25% target to SHOULD and state "not yet evaluated" in
   release notes. Recommended.
2. Keep it as MUST and plan a small study (3-5 reviewers, 20 pages) before
   the 1.0 release.
3. Remove REV-07.

### O-02 Layout analysis method (resolved: D-11, options 1 and 2)

Options, with what they cost:

1. Classical computer vision only: deskew by Hough/projection, connected
   components, run-length smearing, recursive X-Y cut on projection profiles,
   maximal whitespace rectangles for gutters and margins, line clustering by
   baseline. Deterministic, fast, no model files, no license issues, works
   well on clean bilevel 600 DPI scans like the corpus. Weak at semantic
   classification (heading vs. body) beyond geometry and size heuristics.
   Recommended baseline; it is also what "independent image analysis" in
   PIPE-01 asks for.
2. Tesseract's own page segmentation (psm 1 or 3) as a second opinion. Free,
   already present. Tends to merge marginalia into body lines and ignores
   gutters when text nearly touches, which is why the PRD wants independent
   detection.
3. ONNX layout detector via `ort` or `rten`: PP-DocLayout (Apache 2.0,
   PaddleOCR project but usable as a plain ONNX model, ~20-100 MB), or a
   DocLayNet-trained detector. Trained on modern documents; categories are
   text, title, list, table, figure. Marginalia and catchwords are not
   categories, so it helps with figures and tables, not with the hard parts.
   Ultralytics YOLO variants are AGPL and excluded.
4. Historical-document segmenters (Kraken/eScriptorium `blla`, dhSegment,
   Surya): best fit for old print but Python/PyTorch, large, and some carry
   non-commercial or GPL weights. Not practical without a Python runtime.

Recommendation: option 1 in Phase 1, option 2 as extra evidence, option 3
as an optional model pack later for tables and illustrations only.

### O-03 Headers, footers, and marginalia in DOCX (resolved: D-12, option 3 default, option 1 optional)

The PRD requires native Word header/footer stories with one section per
source page. Alternatives:

1. **One section per source page, native stories.** Exact to the PRD.
   529 sections is valid OOXML; Word opens it in a few seconds. The real cost
   is editing: every page ends in a section break, deleting across one merges
   sections and can drop a header, and users see "Section 529" in the status
   bar.
2. **Sections only where furniture changes, Word odd/even headers.** Running
   heads in old books alternate verso/recto, which Word models natively
   ("different odd and even"). One section per chapter or book, PAGE field
   for numbers. Clean documents, but printed page numbers only match if Word
   pagination tracks source pages exactly, which EXP-02 explicitly does not
   promise. Original numbers would have to be shown in the body instead.
3. **Styled body paragraphs.** Header text emitted as a "Running head"
   paragraph style at the top of each page block, page number as a "Printed
   page number" style, marginalia as a "Side note" paragraph style placed
   before the paragraph it annotates, footer text as "Footer" style after
   the body. Fast, trivially editable, searchable, visibly distinct, and
   survives repagination. Not native stories.
4. **Marginalia as a two-column table or anchored text boxes.** Looks like
   the printed page; text boxes are fragile in Word and inaccessible to
   screen readers, tables are robust but awkward to edit.
5. **Drop furniture from the body**, keep it only in the companion archive
   (EXP-05).

Recommendation: make 3 the default and offer 1 as an option ("Native Word
headers and footers, one section per page"). Amend EXP-03 accordingly.

### O-04 Frontend framework inside Tauri (resolved: D-13, Svelte after an initial React choice)

Tauri is settled; the webview framework is not. Recommendation: TypeScript +
React + Vite, because the accessibility tooling needed for UX-05 (Radix or
React Aria primitives, axe-core, Testing Library) is most mature there.
Svelte 5 is the lighter alternative if bundle size matters more.

Outcome: React was chosen first, then replaced by Svelte 5 with Bits UI the
same day (D-13). Both render the mockups identically in WebView2; the
trade is a lighter runtime and less boilerplate against thinner
drag-and-drop and slider libraries, covered by explicit keyboard tests.

### O-05 Preview rendering path (resolved: D-14, PDFium)

Recommendation: render previews in the Rust backend with PDFium at a preview
scale, cache as WebP in the render cache, serve to the webview through a
custom Tauri protocol. One renderer means one coordinate system for
highlights. Alternative: pdf.js in the webview, which is independent of
recognition rendering as IMG-01 asks but doubles the geometry mapping.
