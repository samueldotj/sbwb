# Crate inventory

Candidate Rust crates per architectural layer (NFR-13). Versions are the
latest on crates.io as of 2026-09-12; licenses are from the crate metadata.
"Risk" notes what must be verified before the crate is committed to.

Toolchain present on the reference machine: Rust 1.97.1, Node 24.18, npm 11.16.

## Application shell (Tauri)

| Crate | Version | License | Purpose | Risk |
| --- | --- | --- | --- | --- |
| `tauri` | 2.11 | MIT/Apache | Desktop shell, native menus, IPC, custom protocols | WebView2 is bundled on Win 11; offline installer must include the WebView2 bootstrapper or fixed runtime (NFR-12) |
| `tauri-build` | 2.x | MIT/Apache | Build script | |
| `tauri-plugin-dialog` | 2.7 | MIT/Apache | Native open/save dialogs (Import PDF, Open project, export destination) | |
| `tauri-plugin-window-state` | 2.4 | MIT/Apache | Remember window size and split position | |
| `tauri-plugin-log` | 2.9 | MIT/Apache | Route `log`/`tracing` to files and devtools | Redaction of paths in diagnostics is our job (NFR-11) |
| `tauri-plugin-store` | 2.4 | MIT/Apache | Application settings (recent projects, presets), kept separate from project content (PRJ-05) | |
| `tauri-plugin-single-instance` | 2.x | MIT/Apache | Second launch opens in the running instance; needed for the one-writer rule (PRJ-02) | |
| `keyring` | 4.2 | MIT/Apache | OS credential store for API keys (Phase 2, AI-01) | Windows Credential Manager only; macOS/Linux backends verified later |

Frontend (not crates, D-13): TypeScript, Vite, Svelte 5, Bits UI, svelte-dnd-action, TanStack Virtual,
`@tauri-apps/api`, axe-core for accessibility audits, Playwright + `tauri-driver`
for end-to-end tests.

## PDF access

| Crate | Version | License | Purpose | Risk |
| --- | --- | --- | --- | --- |
| `pdfium-render` | 0.9 | MIT/Apache | Render pages at arbitrary DPI, page geometry, text-layer extraction. Handles JBIG2, which the corpus uses on every page | Requires `pdfium.dll` (~6 MB) from bblanchon/pdfium-binaries (BSD-3/Apache-2) bundled as a Tauri resource and loaded dynamically |
| `lopdf` | 0.45 | MIT | Structural inspection without rendering: page count, media boxes, encryption flag, metadata, page-range extraction for trials, checksum of the original stream | Malformed PDFs: run inside the worker process, not the UI process (NFR-11) |
| `hayro` + `hayro-jbig2` | 0.7 / 0.3 | MIT/Apache | Pure-Rust renderer with a JBIG2 decoder. Fallback if shipping PDFium becomes a packaging problem | Young project; fidelity and speed on 600 DPI JBIG2 not yet measured |

Not chosen: `mupdf` (AGPL), `poppler-rs` (GPL, GTK build chain on Windows).
Both remain permitted by D-04 if PDFium fails.

## Image preparation and evidence

| Crate | Version | License | Purpose |
| --- | --- | --- | --- |
| `image` | 0.25 | MIT/Apache | Decode/encode PNG, WebP (cache), TIFF; pixel buffers |
| `imageproc` | 0.27 | MIT | Deskew (Hough lines, projection profiles), morphology, connected components, integral images, drawing for debug overlays |
| `fast_image_resize` | 6.1 | MIT/Apache | Deterministic Lanczos3 resampling for the fixed 3x marginalia enlargement (OCR-02); records scale and filter |
| `ndarray` | 0.16 | MIT/Apache | Projection profiles, whitespace analysis |
| `rstar` | 0.13 | MIT/Apache | R-tree over regions and words for hit testing and overlap checks (LAY-03) |
| `geo` | 0.33 | MIT/Apache | Rectangle and polygon operations for region editing (LAY-02) |

## Recognition engines

| Crate | Version | License | Purpose | Risk |
| --- | --- | --- | --- | --- |
| `tesseract` | 0.15 | MIT | High-level Tesseract 5 API: set image, TSV output with word boxes and native word confidences, hOCR, page segmentation modes | Builds on `tesseract-sys` (MIT), which on Windows links Tesseract + Leptonica via vcpkg (`x64-windows-static-md`). Tesseract is Apache-2.0. One-time toolchain setup, then static linking, no runtime install |
| `ocrs` + `rten` | 0.13 / 0.26 | MIT/Apache | Optional second engine, pure Rust, detection + recognition, for disagreement evidence (OCR-02) | Model files are distributed separately; confirm their license before bundling. Produces line scores, not word scores; must not be presented as word confidence (OCR-01) |

Model packs: `tessdata` (fast) and `tessdata_best` `eng.traineddata` (Apache-2.0),
downloaded from the tesseract-ocr GitHub releases with recorded SHA-256, or
bundled in the offline installer.

## Text reconstruction

| Crate | Version | License | Purpose |
| --- | --- | --- | --- |
| `spellbook` | 0.4 | MPL-2.0 | Pure-Rust Hunspell-compatible checker; loads the SCOWL/LibreOffice `.dic`/`.aff` fixtures and suggests |
| `symspell` | 0.5 | MIT | Fast edit-distance candidates against project vocabulary and Webster 1913 headwords |
| `fst` | 0.4 | MIT/Unlicense | Compact lexicon sets with fuzzy (Levenshtein automaton) lookup |
| `strsim` | 0.11 | MIT | Damerau-Levenshtein and Jaro-Winkler for candidate scoring |
| `aho-corasick` | 1.1 | MIT/Unlicense | Whole-token multi-pattern search for grouped corrections (REV-05) |
| `unicode-segmentation` | 1.13 | MIT/Apache | Word and grapheme boundaries for anchors |
| `unicode-normalization` | 0.1 | MIT/Apache | NFC handling; long-s and ligature transcription choices stay explicit (TXT-01) |
| `regex` | 1.x | MIT/Apache | Hyphenation and token rules |

`zspell` was considered but has a non-standard license field; `spellbook`
(MPL-2.0, file-level copyleft only) is compatible with an MIT application.

## Persistence and history

| Crate | Version | License | Purpose |
| --- | --- | --- | --- |
| `rusqlite` (feature `bundled`) | 0.40 | MIT | The `.sbwb` project file: WAL mode, transactions for atomic cross-page edits (TXT-02), advisory writer lock (PRJ-02) |
| `rusqlite_migration` | 2.6 | Apache-2.0 | Versioned schema with `user_version`; backup before migration (PRJ-02) |
| `serde` + `serde_json` | 1.x | MIT/Apache | Settings, provenance records, history payloads |
| `blake3` | 1.8 | CC0/Apache | Source PDF and export checksums (PRJ-01, EXP-04) |
| `jiff` | 0.2 | MIT/Unlicense | Timestamps in history and manifests |
| `uuid` | 1.x | MIT/Apache | Stable IDs for regions, spans, decisions |
| `tempfile` | 3.x | MIT/Apache | Write-then-rename for atomic file publication |

## Workflow and workers

| Crate | Version | License | Purpose |
| --- | --- | --- | --- |
| `tokio` | 1.53 | MIT | Async runtime for the Tauri side: job scheduling, progress events, cancellation tokens |
| `rayon` | 1.12 | MIT/Apache | Per-page CPU parallelism inside a worker, bounded thread pool (NFR-07) |
| `ipc-channel` or `interprocess` | 0.23 / 2.4 | MIT/Apache; 0BSD/Apache | Talk to OCR worker processes (same executable, `--worker` mode) so an engine crash or timeout never takes down the UI (NFR-06, NFR-11) |
| `sysinfo` | 0.39 | MIT | Memory and CPU accounting for the 6 GB cap and worker count (NFR-04) |
| `crossbeam-channel` | 0.5 | MIT/Apache | Progress fan-in from workers |

## Second engine (M8.3)

| Crate | Version | License | Purpose | Risk |
| --- | --- | --- | --- | --- |
| `ocrs` | 0.13 | MIT/Apache | Pure-Rust text detection and line recognition for disagreement evidence | Line-level only: no word confidence, so its readings are shown unscored |
| `rten`, `rten-imageproc` | 0.26 | MIT/Apache | Inference runtime and geometry for `ocrs` models | |

## Word export and validation

| Crate | Version | License | Purpose | Risk |
| --- | --- | --- | --- | --- |
| `docx-rs` | 0.4 | MIT | Write DOCX: paragraphs, named styles, headings, tables, images, headers/footers, section properties, page breaks, core properties | Verify custom paragraph styles (default policy) and section-per-page headers (optional policy, D-12) early |
| `zip` | 8.x | MIT | Read the produced package back for validation, build the companion archive (EXP-05) | |
| `roxmltree` | 0.21 | MIT/Apache | Parse `document.xml`, `header*.xml`, `footer*.xml` for parity, duplicate, and linkage checks (EXP-04) | |
| `quick-xml` | 0.42 | MIT | Write PAGE XML and the export manifest (EXP-05) | |

`docx-rust` (MIT) is the alternative if `docx-rs` lacks a needed section
feature; it also reads DOCX.

## Diagnostics, logging, errors

| Crate | Version | License | Purpose |
| --- | --- | --- | --- |
| `tracing` + `tracing-subscriber` + `tracing-appender` | 0.1 / 0.3 | MIT | Structured, rotated logs; spans per job and page |
| `thiserror` | 2.x | MIT/Apache | Typed error boundaries between layers |
| `anyhow` | 1.x | MIT/Apache | Error context in binaries and tests |

## Testing and release tooling

| Tool | Purpose |
| --- | --- |
| `cargo-nextest` | Fast, isolated test runs |
| `insta` (1.48, Apache) | Snapshot tests for layout output, DOCX XML, PAGE XML |
| `proptest` | Property tests for anchors, joins, and undo invariants (A-06, A-08) |
| `criterion` | Benchmarks for NFR-03/05/07 |
| `cargo-deny`, `cargo-about` | License gate and generated notices (NFR-12) |
| `tauri-driver` + Playwright | GUI acceptance scenarios, accessibility audit with axe-core (A-13) |
| `vcpkg` (system tool) | Windows build of Tesseract/Leptonica for `tesseract-sys` |

## Deliberately excluded

- PaddleOCR in any form (D-03).
- `leptess`: unmaintained since 2023, superseded by `tesseract`.
- Python runtime or any crate that shells out to Python.
- `mupdf`, `poppler` unless PDFium proves unusable (D-04).
- ONNX layout models and `ort` (D-11: classical image analysis plus Tesseract segmentation only).
- Ultralytics YOLO layout models (AGPL weights and code).
