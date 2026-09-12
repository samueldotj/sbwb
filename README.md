# sbwb

Scanned Book Work Bench: a local desktop application that turns scanned
historical book PDFs into editable, source-linked Word documents.

Rust core, Tauri 2 shell, Svelte 5 + TypeScript + Vite frontend. Windows 11 x64
is the first qualified platform; macOS and Linux builds stay compiling.
Everything runs offline; optional AI proofreading is a later phase.

## Documents

| Document | Purpose |
| --- | --- |
| [docs/requirements.md](docs/requirements.md) | Product requirements with stable IDs (UX, PRJ, PIPE, IMG, OCR, LAY, TXT, PROV, REV, AI, EXP, NFR) and acceptance scenarios |
| [docs/design.md](docs/design.md) | UI/UX specification derived from the Claude Design mockups: shell, themes and tokens, screens, keyboard map, component inventory |
| [docs/decisions.md](docs/decisions.md) | Decision log D-01 onward, with the options that were considered |
| [docs/crates.md](docs/crates.md) | Rust crate inventory by layer, with licenses and risks |
| [fixtures/README.md](fixtures/README.md) | Reference corpus (Hough 1839) and lexicons, with checksums and licenses |
| [design/README.md](design/README.md) | Where the mockups come from and how to restore the local copy |

## Status

Requirements, design, and stack are settled (2026-09-12). M0 (scaffold) and M1 (projects and import) are complete: a PDF imports into
a `.sbwb` project next to it, reopens with integrity and lock checks, and the
Welcome and import screens match the mockups. See `docs/dev-setup.md` to build locally.

## Roadmap

Milestones are ordered so each one produces something runnable and each
later one builds on verified work. Requirement IDs and design sections are
cited on every task so nothing is built without a home. Estimates assume one
developer; "week" is a rough unit, not a commitment.

### Milestone overview

| Milestone | Goal | Estimate | Status |
| --- | --- | --- | --- |
| M0 | Scaffold and toolchain | 1-2 weeks | Done (CI unverified) |
| M1 | Projects and import | 2 weeks | Done |
| M2 | Rendering and page navigation | 1-2 weeks | Done (benchmarks partial) |
| M3 | OCR pipeline and processing UI | 3 weeks | Done |
| M4 | Layout analysis and Layout mode | 3 weeks | Done (ground truth partial) |
| M5 | Text reconstruction and provenance | 3 weeks | Not started |
| M6 | Review workspace | 4 weeks | Not started |
| M7 | Word export | 3 weeks | Not started |
| M8 | Targeted refinement and second engine | 2 weeks | Not started |
| M9 | Hardening and release | 3 weeks | Not started |
| Phase 2 | Optional AI, queue, other platforms, Early Modern English | — | Not started |

Status values: Not started · In progress · Blocked · Done. Update the row when work starts, not when it is planned.

### M0 — Scaffold and toolchain (1-2 weeks)

Goal: an empty SBWB window opens on Windows from a built installer, with the
crate layout, CI, and design tokens in place.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M0.1 | Cargo workspace: `sbwb-app` (Tauri), `sbwb-core` (domain types, ids, errors), `sbwb-pdf`, `sbwb-image`, `sbwb-ocr`, `sbwb-layout`, `sbwb-text`, `sbwb-review`, `sbwb-store`, `sbwb-export`, `sbwb-worker` (worker binary entry). Typed boundaries | NFR-13 | Done |
| M0.2 | Frontend: Vite + Svelte 5 + TypeScript, Bits UI primitives, svelte-dnd-action, TanStack Virtual, `@tauri-apps/api`, ESLint, svelte-check, Vitest, Testing Library for Svelte | D-13, UX-05 | Done |
| M0.3 | Design tokens as CSS custom properties for Paper and Bench, `data-theme` switch, bundled OFL fonts | design 2, D-17, D-18 | Done |
| M0.4 | Shell components: custom title bar, status bar, pipeline rail skeleton, inspector frame, sheet, toast | design 3, UX-01 | Done |
| M0.5 | Windows build of Tesseract + Leptonica via vcpkg; `tesseract-sys` linking proven with a smoke test that OCRs one fixture crop | OCR-01, docs/crates.md | Done |
| M0.6 | PDFium binary fetch script with checksum, loaded by `pdfium-render`; render one fixture page in a test | D-14, docs/crates.md | Done |
| M0.7 | CI: `cargo nextest`, `cargo clippy -D warnings`, `cargo deny`, `cargo about` notices, Vitest, Tauri bundle to MSI/NSIS | NFR-12, NFR-13 | In progress |
| M0.8 | Logging with `tracing` to rotated files, path redaction hook, opt-in diagnostics bundle stub | NFR-11 | Done |
| M0.9 | Worker process mode: same executable launched with `--worker`, IPC channel, heartbeat, kill after grace | NFR-06, NFR-11 | Done |

Exit: installer builds on CI, app opens to an empty Welcome page in both themes, axe-core reports no violations on the shell.

### M1 — Projects and import (2 weeks)

Goal: import a PDF into a `.sbwb` project, see it on the Welcome page, reopen it.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M1.1 | SQLite schema v1 with `rusqlite_migration`: project meta, source blob + checksum, pages, page scope, settings, history, export log | PRJ-01, PRJ-02, D-09 | Done |
| M1.2 | Import validation: readability, page count, encryption prompt, disk space, size limits, actionable errors | PRJ-01 | Done |
| M1.3 | Source integrity check on open and before dependent operations; mismatch blocks with explanation | PRJ-01 | Done |
| M1.4 | Page scope model: processed range separate from source count; extend without re-import | PRJ-03 | Done |
| M1.5 | Save, save a copy (consistent portable copy), close, single-writer lock with read-only fallback | PRJ-02, PRJ-04 | Done |
| M1.6 | Welcome page: drop zone, Choose PDF, Open project, recent books with scope/stage/approval bar, Continue | design 4.1 | Done |
| M1.7 | App settings store separate from project content; recent list clearing does not delete projects | PRJ-05 | Done |
| M1.8 | Native File menu wiring | UX-01 | Done |
| M1.9 | Tests: import fixture `pages-001-110.pdf`, reopen, checksum mismatch, locked project | PRJ-01, PRJ-02 | Done |

Exit: A-01 through import step; project survives close and reopen.

### M2 — Rendering and page navigation (1-2 weeks)

Goal: browse the imported book smoothly in the scan pane.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M2.1 | Render service: PDFium worker renders at requested scale, WebP cache directory next to the project with LRU cap | IMG-01, D-14, NFR-04 | Done |
| M2.2 | Custom Tauri protocol serving cached renders; coordinate transform helpers PDF points ↔ bitmap pixels | PROV-01 | Done |
| M2.3 | Scan pane: header controls, fit/zoom, tiled re-render above cap, pan, previous page kept until next is ready, stale-response guard | UX-03, design 4.3 | Done |
| M2.4 | Filmstrip and rail page mini-grid with status dots; page grid for Processing view | design 4.2, 4.3 | Done |
| M2.5 | Keyboard navigation for pages and zoom | design 5 | Done |
| M2.6 | Benchmarks: cached navigation p95 ≤ 250 ms, uncached ≤ 2 s on the reference machine | NFR-03 | In progress |

Exit: A-13 navigation portion passes; no blank flicker between cached pages.

### M3 — OCR pipeline and processing UI (3 weeks)

Goal: import triggers OCR automatically with live progress, pause, resume, retry.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M3.1 | Job model: plan with stages Import → OCR → Layout → Text pass, per-page units, recorded algorithms/models/settings/input revisions | PIPE-01 | Done |
| M3.2 | Scheduler on Tokio: bounded worker pool, per-unit timeout 120 s, cancellation at page boundaries, escalation after 10 s grace | NFR-06, NFR-07 | Done |
| M3.3 | Tesseract adapter: page render at 300 DPI, `eng` and `tessdata_best/eng`, TSV parse into words with boxes and native confidences, hOCR kept as raw evidence | OCR-01, PROV-01 | Done |
| M3.4 | Preparation: orientation, deskew, conservative contrast; originals and variants recorded with geometry | IMG-01 | Done |
| M3.5 | Model pack manager: catalog with checksums, download or local package install, offline bundle of `eng` | OCR-01, NFR-12 | Done (no downloader; both packs bundled) |
| M3.6 | Progress events: stage, done/total, elapsed, estimate when reliable, activity heartbeat ≤ 2 s; log lines | PIPE-02, design 4.2 | Done |
| M3.7 | Pipeline rail live state, Pause all / Resume / Retry failed pages, job monitor with per-stage cards and log | design 3, 4.2 | Done |
| M3.8 | Import inspector: source card, First 50 / Range… / All, estimate, Start reviewing page 1, project location | design 4.2, UX-02 | Done |
| M3.9 | Settings page, "this book · Processing" rows and app groups; apply with re-run consequence | UX-06, design 4.7 | Done |
| M3.10 | Memory accounting with `sysinfo`, caps on queues and crops, oversized-page warning | NFR-04 | Done |
| M3.11 | Durability tests: kill worker mid-page, disk full, restart and resume | NFR-10 | Partial (timeout, cancel, resume tested; disk-full pending) |

Exit: A-02 extend-a-trial passes; four workers reach ≥ 2× single-worker throughput or a documented decision (NFR-07).

### M4 — Layout analysis and Layout mode (3 weeks)

Goal: regions, columns, marginalia, and reading order detected and correctable.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M4.1 | Classical analysis: connected components, projection profiles, maximal whitespace rectangles for gutters and margins, baseline clustering, deskew confirmation | LAY-01, D-11 | Done |
| M4.2 | Tesseract page segmentation recorded as second evidence; disagreement flagged | D-11 | Done |
| M4.3 | Region model and classification: body, heading, header, footer, marginalia, footnote, page number, catchword, illustration, table, uncertain; heuristics from position, size, and line height | LAY-01 | Done |
| M4.4 | Reading order builder that follows structure, never joins across a gutter on the same band | LAY-01 | Done |
| M4.5 | Coverage accounting: uncovered text-like areas, clipping, overlaps, empty-area suspicion | LAY-03 | Done |
| M4.6 | Layout mode UI: tools V/R/X/M, region overlay with tags and handles, reading-order list with placement, region inspector, Revert / Save layout | LAY-02, design 4.4 | Done |
| M4.7 | Layout edits invalidate only dependent results; impact preview before rerun; undo | LAY-02, PRJ-05 | Done |
| M4.8 | Ground truth for 20 fixture pages (regions and order) and metrics: region coverage, missed lines, column/order errors | NFR-08 | Partial (10-page draft, 1 adjudicated) |

Exit: A-03 two-column/marginalia scenario passes on the annotated pages.

### M5 — Text reconstruction and provenance (3 weeks)

Goal: logical paragraphs, joined hyphens, scored candidates, auto-apply policy, full source mapping.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M5.1 | Token and span model with anchors to source regions and revisions; edits never inherit original confidence | PROV-01 | Not started |
| M5.2 | Paragraph builder from indentation, spacing, reading order, user breaks | TXT-02 | Not started |
| M5.3 | Cross-line and cross-page hyphen joins with atomic multi-fragment transactions and undo guards | TXT-02, PRJ-05 | Not started |
| M5.4 | Lexicon service: Hunspell via `spellbook`, Webster 1913 headword FST, project vocabulary and protected words | TXT-01, D-07 | Not started |
| M5.5 | Candidate generation and heuristic scoring; long-s, ligature, punctuation, names, compounds treated conservatively | TXT-01 | Not started |
| M5.6 | Auto-apply policy: threshold 90, eligible rule, valid anchors, unchanged revision, no protected decisions; applied changes distinguishable and undoable | TXT-03 | Not started |
| M5.7 | History store: durable, named entries, guarded undo | PRJ-05 | Not started |
| M5.8 | Text pass inspector tab: slider, counters, Re-run, Review N | design 4.6 | Not started |
| M5.9 | Held-out precision measurement harness for auto-corrections | NFR-09 | Not started |

Exit: A-05 and A-06 pass; auto-apply ships as suggestion-only if precision < 99%.

### M6 — Review workspace (4 weeks)

Goal: the keyboard-first review loop from the mockups, end to end.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M6.1 | Issue index: types (missing text, order ambiguity, clipping, conflicting readings, risky substitutions, questionable joins, user flags), priority, counts by state, stale detection | REV-01 | Not started |
| M6.2 | Threshold filter (default 70), category filters, hide completed/auto-applied/approved markers | REV-02 | Not started |
| M6.3 | Transcript pane: logical paragraphs, running-head line, marks with hover/focus source highlighting | design 4.3, REV-03 | Not started |
| M6.4 | Issue tab: evidence crop, reading, score chip with source, reason, candidates 1-9, Accept/Edit/Skip/Later, history, auto-advance | REV-03, design 4.3 | Not started |
| M6.5 | J/K navigation across the book, deduplicated cross-page items, wrap once, failures reported | REV-04 | Not started |
| M6.6 | Grouped corrections: whole-token matching, preview with exclusions and conflicts, atomic apply, named history, guarded undo | REV-05 | Not started |
| M6.7 | Decisions anchored to revisions; rerun protection; deferred view; page approval with covered revision and outstanding acknowledgements; Approve page button | REV-06 | Not started |
| M6.8 | Page tab: no matching / no unresolved / approved states, exclusions, re-run | REV-06 | Not started |
| M6.9 | Drafts and saving: transactional commits, Saving/Saved/Save failed, guarded navigation, close dialog, crash recovery | PRJ-04 | Not started |
| M6.10 | Accessibility pass: names, focus order, announcements, enlarged text, contrast | UX-05 | Not started |
| M6.11 | Indexed next-issue query p95 ≤ 300 ms on 10,000 issues | NFR-03 | Not started |

Exit: A-07, A-08, A-09, A-10, A-13 pass.

### M7 — Word export (3 weeks)

Goal: working and clean copies with validated structure and a report.

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M7.1 | Export snapshot: immutable selection of text, layout, approvals, mappings, settings | EXP-04 | Not started |
| M7.2 | DOCX writer with `docx-rs`: styles, logical paragraphs, heading levels, simple tables, page breaks, core properties | EXP-01, EXP-02 | Not started |
| M7.3 | Furniture policies: styled paragraphs default; native headers/footers with section per page as option; cross-page word boundary rule | EXP-02, EXP-03, D-12 | Not started |
| M7.4 | Inclusion policies: page numbers, footnotes, catchwords, illustrations as images, uncertain regions, tables as image or text with warning | EXP-03 | Not started |
| M7.5 | Working copy annotations: highlights and one comment per flag; clean copy gate on approval | EXP-01 | Not started |
| M7.6 | Validation: read back with `zip` + `roxmltree`, structure, coverage, duplicates, boundary policy, header linkage, text parity; atomic publish with checksum and report | EXP-04 | Not started |
| M7.7 | Export sheet UI: readiness counters, scope radios, page structure, checkboxes, archive details, progress, previous exports | design 4.5 | Not started |
| M7.8 | Formatting presets and metadata: page size, margins, fonts with substitution warning, drop caps, PDF metadata view vs document properties | EXP-06 | Not started |
| M7.9 | Archive bundle: transcript/source map JSON, PAGE XML, manifest, validation report | EXP-05, SHOULD | Not started |
| M7.10 | Export fixture of 250,000 words with notes and two-column pages; target 60 s | NFR-05 | Not started |

Exit: A-12 passes under both furniture policies; A-01 passes end to end.

### M8 — Targeted refinement and second engine (2 weeks)

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M8.1 | Region OCR from a drawn or detected region: 300 DPI crop, fixed 3× Lanczos enlargement, recorded inputs, deterministic profile | OCR-02 | Not started |
| M8.2 | Coverage-gap detection feeds the issue index as unscored structural issues | LAY-03, REV-01 | Not started |
| M8.3 | Optional `ocrs` second engine for disagreement evidence; line scores never shown as word scores | OCR-01, OCR-02 | Not started |
| M8.4 | Candidate merging into the existing review without replacing human-approved text | OCR-02 | Not started |

Exit: A-04 passes.

### M9 — Hardening and release (3 weeks)

| ID | Task | References | Status |
| --- | --- | --- | --- |
| M9.1 | Benchmark suite on the reference machine with recorded hardware, model versions, hashes | NFR-02, NFR-03, NFR-05, NFR-07 | Not started |
| M9.2 | Full 529-page run with resource logging; resume after interruption | A-14, NFR-04 | Not started |
| M9.3 | OCR/coverage quality gates frozen from the annotated corpus | NFR-08 | Not started |
| M9.4 | Durability matrix: crash, worker exit, disk full, interrupted export | NFR-10 | Not started |
| M9.5 | Offline installer test on a clean Windows VM with networking disabled | A-01, NFR-12 | Not started |
| M9.6 | Code signing, license notices, dependency and model inventories | NFR-12 | Not started |
| M9.7 | Accessibility audit of core flows with axe-core and a screen reader | UX-05 | Not started |
| M9.8 | User documentation: shortcuts, furniture policies, limitations, release notes listing undelivered SHOULD items | NFR-12, section 12 | Not started |

Exit: all MUST requirements and scenarios A-01 to A-14 have evidence.

### Phase 2

| ID | Task | References | Status |
| --- | --- | --- | --- |
| P2.1 | AI providers: OpenAI, Anthropic, Google adapters; keys via `keyring`; consent card; budgets; run history; suggestions into the review inbox, never auto-applied | AI-01..03, design 4.6 | Not started |
| P2.2 | Multiple-book queue | PRJ-06 | Not started |
| P2.3 | macOS and Linux qualification, notarization, Linux verification info | NFR-01 | Not started |
| P2.4 | Early Modern English lexicon and evaluation fixture | TXT-01, D-06 | Not started |

## Working conventions

- Every task references at least one requirement ID or design section.
- Decisions that are not derivable from the code go into `docs/decisions.md`.
- Mockup exports stay local; UI changes are specified in `docs/design.md` first.
- Large fixtures are git-ignored and documented with checksums in `fixtures/README.md`.
