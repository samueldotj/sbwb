# Release notes

## 0.3.0 (2026-10-23)

Opening a 0.2.0 project upgrades its file (schema 8 → 10) after saving a
backup beside it (`.pre-v9.bak`, `.pre-v10.bak`); 0.2.0 cannot open an
upgraded project.

### Transcript

- Click a word for a menu under it: link it to one of the page's
  footnotes (listed by their first characters), spelling suggestions,
  grammar fixes, Roman numerals, **Edit text**, and paragraph tools.
  Double-click still edits the word straight away.
- **Spelling** in UK or US English (Highlight menu): words in neither the
  dictionary, the Webster 1913 list nor the book's own words get a wavy
  red underline, with suggestions and **Add to this book**. Both SCOWL
  dictionaries are bundled; nothing goes online.
- **Grammar** (Highlight menu): blue underlines for extra spaces, a
  missing space after punctuation, a space before punctuation, quotation
  mark spacing, a lower-case word after a full stop, and repeated words,
  most with a one-click fix.
- **Roman numerals**: the menu shows a numeral's value with **Convert**
  to a number, and for a misread one ("1V.", "VIL", "Ixvii.") the
  numerals it could have been.
- **Paragraphs**: start a new paragraph at a word, or join a paragraph to
  the one above (the line break goes too). Undoable, exported, and kept
  when the text pass re-runs the page.
- **Footnotes of several paragraphs**: mark a footnote paragraph as
  continuing the note above; it goes into the same Word footnote.
- **Highlight** numbers, ordinary punctuation and special characters.
- Text outside every layout region is hidden by default (**Show
  outside**).
- Hovering any word highlights it on the scan; corrected words can be
  tracked (**Corrections**) and revisited, and an issue's candidates open
  at the word.

### Review

- **Ctrl+Z** undoes the last change - a decision, text edit, grouped
  correction, approval or paragraph change - one step back per press,
  opening the page it was on.
- Approving a page names the open issues it would acknowledge, and can
  happen by itself once a page's issues are resolved (**Auto-approve page
  when resolved**).
- A book reopens where it was left: mode, page, inspector tab and zoom.

## 0.2.0 (2026-10-19)

Opening a 0.1.0 project upgrades its file (schema 6 → 8) after saving a
backup beside it (`.pre-v7.bak`, `.pre-v8.bak`); 0.1.0 cannot open an
upgraded project.

### Review and text

- Select any words in the transcript and choose **Edit text**: one
  undoable change, each typed word kept on the scan word it replaces, and
  re-applied after a text-pass rerun.
- **Edit & accept** starts from the chosen suggestion; the "Also replace
  elsewhere?" offer only appears when there is something to replace.
- Footnote references: link a word to a footnote in the Text pass tab, or
  accept links found from printed markers (*, †, digits, and the "!" / "?"
  OCR makes of a dagger). Numbers are automatic per page; linked notes
  export as real Word footnotes.
- No corrections are offered for text outside every layout region.
- Transcript zoom (70–400%, Ctrl+wheel); highlights on the scan are
  outlines so the word stays readable.

### Layout and OCR

- Layout mode: select several regions (drag a rectangle, or Ctrl/Shift-
  click) and delete them or change their type together.
- Recognise region now reads small, heavy side notes the page OCR
  skipped (single-block segmentation), and places them in reading order.
- OCR and Layout tabs, opened from the pipeline rail: engine settings,
  progress, failed pages, the weakest pages by OCR confidence, and the
  pages whose layout needs a look.
- The job monitor is folded into the pipeline rail (D-34): live speed and
  time left under the running stage, the latest log lines, and the full
  log in a sheet. Live progress and the OCR count now show correctly.

### Pages view and export

- Thumbnail size slider (Ctrl+wheel); previews render on several workers
  and for the whole book in the background, with Stop and Generate.
- Export: page size presets (A4, Letter, A5, book trims from 5 × 8 to
  7 × 10 in, the scan's own size, or custom), margin presets with four
  separate margins, warnings at the bottom, and **Open document**.
- Word export puts running heads, page numbers and footers in Word headers
  and footers and side notes in boxes; Word comments can be left out of the
  working copy.

### Fixes

- Layout edits are no longer discarded by a region reload loop; layout
  autosaves.
- Long names wrap in the Import panel.

## 0.1.0 (2026-09-12) — first Windows release candidate

Delivered (M0–M9): import with page scope, offline Tesseract OCR with two
model packs, layout analysis with editable regions and reading order, the
text pass with hyphen joins and auto-applied confusions at 99% adjudicated
precision (Claude-drafted, user spot-check pending), the review workspace
with a book-wide issue inbox, grouped corrections, approvals and guarded
undo, region OCR at 300 dpi ×3 with an optional second engine, and Word
export under two furniture policies with read-back validation and an
archive bundle.

### Qualification evidence

- Benchmarks: `docs/benchmarks/2026-09-12-sam-desktop2025.md` (debug build on an 8-core Ryzen: 4 workers 2.98× one worker; next-issue p95 0.1 ms; uncached preview p95 129 ms; 250k-word export 7.6 s; the 529-page scan resumed after an interruption with 0 pages recognised twice and 977 MB peak).
- Quality gates: `crates/sbwb-text/tests/quality_gates.rs`,
  `crates/sbwb-layout/tests/groundtruth.rs`, `crates/sbwb-text/tests/precision.rs`.
- Durability: `docs/durability.md`.
- Accessibility: axe-core audit of the review workspace (0 violations after
  the contrast and target-size fixes); screen-reader walkthrough pending.
- Inventory and notices: `docs/inventory.md`, `THIRD-PARTY-NOTICES.md`.

### Phase 2 additions (2026-09-12)

- Optional AI proofreading with OpenAI, Anthropic, and Google adapters
  (AI-01..03): consented, budgeted, validated, suggestion-only. Verified
  against a stand-in server; live provider runs need a key.
- Book queue (PRJ-06): several PDFs, one project each, processed in order.
- Early Modern English profile (TXT-01, D-06) with an evaluation fixture.

### Not delivered in this release (SHOULD items and qualification tasks)

| Item | Requirement | Status |
| --- | --- | --- |
| Native Word footnotes with callouts | EXP-03 | Footnotes are labelled paragraphs |
| Word table cells | EXP-01 | Tables export as text, image, or are skipped |
| Word pagination check | EXP-04 | Reported as unchecked in every validation report |
| macOS and Linux qualification | NFR-01 | Compile only |
| Code signing of the Windows installer | NFR-12 | Needs a certificate; unsigned build |
| Offline install test on a clean VM | A-01, NFR-12 | Pending; `SBWB_0.1.0_x64-setup.exe` (39.6 MB, sha256 525efd5f…) and `SBWB_0.1.0_x64_en-US.msi` (46.7 MB, sha256 78221563…) build and the release binary starts with its bundled resources on the development machine |
| Screen-reader walkthrough | UX-05 | Pending |
| User spot-check of the ground-truth drafts | D-26 | 5 pages of layout and the auto-correction file are waiting for adjudication |

### Known limitations

See `docs/user-guide.md` section 6.
