# Release notes

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
