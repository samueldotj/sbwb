# SBWB UI/UX design specification

Derived from the Claude Design mockups "Book Scanner Redesign" (ten artboards,
local copy in `design/sbwb-redesign/`, not committed, see D-15/D-16). This
document is the versioned source of truth for the interface; the mockups are
the visual reference. Where this document and `requirements.md` disagree, this
document wins for UI and UX (D-15). Requirement IDs are cited so each screen
can be traced back.

## 1. Principles (from the design brief)

- Scan and transcript are always on screen while a book is open.
- The pipeline is a persistent left rail that doubles as a status monitor.
- Every setting lives in a contextual right inspector, not in a wizard.
- Explanatory copy moves into tooltips and one-line hints, never paragraphs.
- Review is keyboard-first: J/K next/previous issue, A accept, E edit,
  S skip, L later, digits pick a candidate, Cmd/Ctrl+Enter approves the page,
  L toggles layout mode from the rail, Esc returns to Review.
- One primary action per pane. Destructive or costly actions state their
  consequence next to the button ("Apply · re-run 19 pages").

## 2. Themes and tokens (D-17, D-18)

Two themes share one layout skeleton. Paper is the default light theme;
Bench is the dark theme. Tokens are CSS custom properties on `:root`,
swapped by `data-theme`.

### Paper (light, default)

| Token | Value | Used for |
| --- | --- | --- |
| `--bg` | `#f4efe6` | workspace and inspector background |
| `--chrome` | `#ece6da` | title bar, status bar, pipeline rail |
| `--strip` | `#e6dfd2` | filmstrip, pane headers |
| `--scan-bg` | `#cfc7b8` | scan pane ground (border `#bfb6a5`) |
| `--paper` | `#fbf8f2` | cards, transcript, inputs |
| `--raised` | `#f8f4ec` | selected rail row, kbd |
| `--border` | `#d9d1c2` | primary borders |
| `--border-soft` | `#e8e1d4` | inner dividers |
| `--border-input` | `#cfc6b6` | buttons and inputs |
| `--track` | `#e0d8c9` | progress tracks, unseen pages |
| `--text` | `#2a2622` | body text, primary buttons |
| `--text-2` | `#5d554b` | secondary text |
| `--muted` | `#8a8177` | labels, hints |
| `--disabled` | `#b3aa9c` | disabled, separators |
| `--accent` | `#b8702e` | running stage, current page, issue focus |
| `--accent-bg` | `#f7e2c6` (`#f4dfc6` on scan) | issue highlight |
| `--accent-text` | `#8a4d14` | issue counts |
| `--ok` | `#4f7d4a` | done, approved, accepted |
| `--ok-bg` | `#e4eddf` (`#d6e6cf` on scan) | accepted highlight |
| `--ok-text` | `#365a32` | |
| `--warn` | `#d9a441` | pages with issues |
| `--unseen` | `#c9b9a3` | pages not yet reviewed |
| `--region-header` | `#6a5acd` | header/page-number region outlines |
| `--scan-paper` | `#f7f2e7` | rendered page backdrop |

Fonts: UI `Source Sans 3` 13px; transcript and titles `Newsreader`
(transcript 15.5px / 1.75); monospace `ui-monospace` for paths and kbd.

### Bench (dark)

| Token | Value | Used for |
| --- | --- | --- |
| `--bg` | `#16181b` | workspace |
| `--chrome` | `#0f1113` | title bar, status bar, log |
| `--panel` | `#1a1d21` | rail, inspector, cards |
| `--panel-header` | `#1e2126` | pane headers, transcript |
| `--raised` | `#22262c` | selected rows, inputs |
| `--border` | `#26292e` | primary borders |
| `--border-input` | `#2c3037` | buttons and inputs |
| `--border-muted` | `#3a4048` / `#4a5058` | drop zone, disabled |
| `--scan-bg` | `#2a2d31` | scan pane ground |
| `--text` | `#e6e8eb` | body text |
| `--text-2` | `#b5bac2` | secondary text |
| `--muted` | `#8b919a` | labels, hints |
| `--accent` | `#6ea0e6` | running stage, focus, primary buttons (text `#0f1113`), 6-8px glow |
| `--accent-text` | `#9dc0f0` | score chips |
| `--ok` | `#5cc48f` (`#3b7a5a` in page grid) | |
| `--warn` | `#d9a441` (`#b08a3a` in page grid) | |
| `--scan-paper` | `#f1ece2` | rendered page backdrop |

Fonts: UI `Instrument Sans` 12.5px; section labels and numbers
`JetBrains Mono` 10-10.5px uppercase with 0.06-0.08em tracking; transcript
`Instrument Sans` 14px / 1.7.

Both themes: primary buttons are filled (`--text` on Paper, `--accent` on
Bench), secondary are outlined with `--border-input`, destructive actions add
a warning colour and never rely on colour alone (UX-05). Radii 4-6px on
controls, 8-12px on cards and sheets. Focus rings use `--accent` at 2px.

## 3. Application shell

Window: custom title bar (Tauri `decorations: false`), 38px on Paper, 34px on
Bench. Three columns: left the app mark "S" plus "SBWB" and an optional
context ("Local · nothing uploaded", "/ Settings"); centre the book title with
volume and scope ("Vol. 1 · 50 of 529 pp"); right the window controls.

Status bar, 26px / 24px: left the running stage with a spinner ("Text pass ·
page 31 of 50 · 62%"), then counters ("122 applied · 250 suggested"); right
the save state ("Saved 2 s ago", "Saving…", "Save failed") and the project
file name. PRJ-04 states map to this bar.

Body: a persistent left **pipeline rail** and a mode-specific centre plus a
right **inspector**. Modes:

| Mode | Entered by | Artboards |
| --- | --- | --- |
| Welcome | app start, closing a book | 1b, 1g |
| Processing | import, clicking a running stage in the rail | 1c, 1h |
| Review | opening a page, default once a book is open | 1a, 1f |
| Layout | "Edit layout" or L, Esc returns to Review | 1d |
| Export | "Export" in the rail, a sheet over the workspace | 1e |
| Settings | title bar / Welcome footer link | 1j |

Narrow windows (below 1100px) stack scan over transcript and collapse the
rail to icons; the inspector becomes a drawer (UX-03).

### Pipeline rail (168-200px)

Rows: Import, OCR, Layout, Text pass, AI proofread, Export. Each row has a
state dot (done = filled `--ok`; running = accent ring spinner with a thin
progress bar under the row; queued/off = hollow), a label, and a trailing
value ("50 pp", "done", "62%", "14 / 50", "off", "—"). The running row is
raised. Below a divider, a **Review** block: Unresolved, Below N%, Pages
approved with a stacked bar (approved / issues / unseen). Bench replaces the
filmstrip with a 10-column page mini-grid in the rail. Footer: keyboard hints
and, in Processing, a "Pause all" button (PIPE-02).

Clicking a rail row opens that stage's inspector tab (Text pass, AI) or the
Processing view (Import, OCR, Layout) or the Export sheet.

## 4. Screens

### 4.1 Welcome (1b Paper, 1g Bench) — UX-01

Paper: 400px left column with the headline "Scanned books, into Word.", a
one-line privacy statement, a dashed drop zone ("Drop a PDF here", "Choose
PDF…", hint "OCR starts right away. You pick the page range next."), an
"Open a .sbwb project…" button, and a footer with version, Settings, and
Shortcuts. Right: "Recent books" list, each card with a thumbnail, title and
volume, a status line ("50 of 529 pages · Text pass running · 31 pages
approved") with the stacked progress bar, and a Continue button on the most
recent. Stored-locally note on the section header.

Bench: heading "Books" with "Open .sbwb…" and primary "Import PDF" on the
right, a full-width dashed drop strip, and a table: title (with path in
mono), pages, progress bar, stage, opened.

Recent entries never delete projects (PRJ-05). Opening never starts
processing or network calls (PIPE-02).

### 4.2 Import and processing (1c Paper, 1h Bench) — UX-02, PIPE-02, PRJ-03

Import starts OCR immediately on the default scope (first 50 pages) so the
user sees progress within seconds. The right **import inspector** (340px)
shows: Source card (file name, page count, size, "integrity verified",
PRJ-01); "Pages to process" with segmented choice First 50 / Range… / All N,
the hint "Start small to check quality. You can extend the range later
without re-importing.", and an estimated time labelled as an estimate; "Now
running" card with stage, per-page rate, progress bar, the note that layout
and text pass follow automatically, and a primary "Start reviewing page 1";
and the project location with a Change link.

Centre: page grid, 10 per row, 3:4 tiles with a status dot (done, running
ring, queued dashed). Header: "Pages · Processing 1–50 · 479 untouched in the
source" and "Click a page to open it while OCR continues". Bench adds an
"Extend range…" button and a 360px expanded job monitor: per-stage cards
(Import "529 pp · 4s", OCR "14/50 · 3.2s/p" with settings line "eng · 300dpi ·
deskew on · ~2m left", Layout queued, Text pass "queued · ≥90%", AI proof
"off · enable") and a **log** in mono with timestamps ("07:41:05 ocr p.14
running", "warn p.11 skew 1.4° corrected", "import ok · 529 pp · sha256
verified"). Failed pages appear in the log and as red-dot tiles with "Retry
failed pages" in the rail (PIPE-02).

### 4.3 Review workspace (1a Paper, 1f Bench) — UX-03, UX-04, REV-01..06

Columns (Paper): rail 168 | filmstrip 76 | scan 1fr | transcript 1fr |
inspector 292. Bench: rail 200 | scan 1fr | transcript 1fr | inspector 300.

**Filmstrip** (Paper): 44x58 page thumbnails with a status dot (approved,
issues, unseen), the current page enlarged with an accent border, legend at
the bottom. Bench uses the rail mini-grid instead.

**Scan pane** header (40px): ‹ "Page 48" of 50 ›, then Fit · 100%, then a
page-label chip ("even · 1 col"). Bottom-left overlay toggles "Regions" and
"Edit layout". The page renders on `--scan-paper` with a drop shadow. Region
outlines: body dashed `--ok`, marginalia filled `--accent-bg` with an accent
outline, header lavender. Word highlights mirror the transcript marks
(accepted green, issue amber, focused issue with a double ring). Bench shows
a hover readout ("line 14 · x 41% · zoom to word"). Zoom is re-rendered by
the backend (D-14); page changes keep the old page until the new one is
ready (UX-03).

**Transcript pane** header: "Transcript", "12 issues on this page", and the
review threshold slider on the right ("Show issues below 70%"). Body: a
"Running head" label line in small caps ("History of Christianity · chap. i.
· 6"), logical paragraphs in the transcript font, footnotes after a divider.
Marks: accepted (green underline), issue (amber underline), focused issue
(amber with ring). Hover or focus on a mark highlights its source word
(REV-03); clicking selects it in the inspector.

**Inspector** tabs: Issue | Page | Text pass | AI.

Issue tab: "Issue 3 of 12 on page 48" with "161 in book"; an evidence card
with a crop strip of the scan showing the word in context, the current
reading in the transcript font, a score chip ("64% · OCR", Bench "0.64
ocr"), and a reason line ("Hyphen joined across line break · comma
uncertain"); a **Candidates** list with score and a numbered kbd (1, 2, 3;
"raw" for the original OCR); decision buttons **Accept (A)**, **Edit (E)**,
**Skip (S)**, **Later (L)**; an "Auto-advance to next issue" checkbox; Bench
adds a **history** list ("text pass joined hyphen · auto", "OCR read
'com-merce' · raw"). Footer: "Page 48 · 9 unresolved" and **Approve page
(Cmd+Enter)**. Skip resolves the exact proposal by keeping the original;
Later defers (REV-03, REV-06). Selecting a candidate is not a commit.

Page tab: page-level state (no matching / no unresolved / approved), printed
page label, region summary, exclusions, re-run controls (REV-06, LAY-03).

### 4.4 Layout mode (1d) — LAY-02

Columns: rail 168 | scan 1fr | reading order 320 | region inspector 264.
Scan header adds tools **Select (V)**, **Draw (R)**, **Split (X)**,
**Merge (M)** and a column count. Regions show numbered type tags ("4 body
· selected", "42 marg", "3 header", "52 footnote"), the selected region has
corner handles, and a dashed gutter line marks the detected column boundary.

**Reading order** list: drag handle, type chip, first line of text, and a
trailing placement ("→ Word header", "archive only", "→ footer note",
"→ footnote", "29 lines"); drag to reorder. A hint explains that body lines
are grouped into one region and split only where Word structure differs.

**Region inspector**: Type chips (body, header, marginalia, footnote, page
no., ignore; plus catchword, illustration, table, uncertain from LAY-01,
shown in an overflow row), **In Word** radios (Ordinary text, Heading 1·2·3,
Table cell), Bounds as x/y/w/h percentages, "Split after line…" and "Merge
with next", the consequence note "Changing a body region re-runs the text
pass for this page and clears its approval", footer **Revert** and **Save
layout**. Status bar: "Layout · page 48 · unsaved changes" and "Esc returns
to Review".

### 4.5 Export sheet (1e) — EXP-01..06

A 720px sheet over the blurred workspace. Header "Export to Word" with the
book and scope ("pages 1–50"), Esc to close. Readiness counters: pages
approved (31 / 50, green), unresolved below threshold (161, amber), AI
suggestions pending (48). **What to export**: radio "Working copy" (Current
text as-is. Unresolved words highlighted, one comment per flag. "ready now")
and "Clean copy" (No annotations. Requires every page in scope approved. "19
pages left"). **Page structure**: segmented "Mirror the scan" / "Continuous
text" with a one-line explanation of the current furniture policy
(default: running heads, page numbers, side notes, and footnotes as styled
paragraphs; native headers/footers when that option is chosen, D-12) and
checkboxes Marginal notes, Footnotes, Source page numbers. A collapsed
`details` "Archive bundle & flag threshold" (PAGE XML, JSON source map,
snapshot, validation report; flag unresolved below 90% in metadata). A
"Previous exports" line. Footer: destination path, Cancel, primary
"Export working copy" / "Export clean copy". Progress replaces the footer
during export (EXP-04).

### 4.6 Text pass and AI tabs (1i) — TXT-03, section 8

Text pass tab: title and one-line description; an "Auto-apply at or above"
slider defaulting to 90% with the note "Below this, changes stay as
suggestions. Human-edited or approved text is never touched."; three
counters applied / suggested / undone; buttons "Re-run pass" and primary
"Review 250 →"; a last-run line.

AI tab (Phase 2, D-05): in Phase 1 the tab and the rail row render in an
"off" state with an "enable" link that explains AI proofreading arrives in a
later version. The Phase 2 layout is: toggle, Provider and Model selects,
Key status ("Connected · kept on this computer", Change), Pages segmented
(This page / 1–50 / Range…), a "what leaves this machine" card with the
consent checkbox, primary "Send 1 page", and a last-run line with "Review
48 AI suggestions →" (AI-01..03).

### 4.7 Settings (1j) — new UX-06

Left nav 220px in two groups: **this book** (Processing, Review, Export
defaults) and **app** (AI providers, Appearance, Shortcuts, Storage &
privacy); footer shows core and engine versions. Content column max 720px,
rows in a bordered list: label plus hint on the left, control on the right
(select, segmented, toggle, slider). Processing rows: OCR language, Render
resolution (200 / 300 / 400), Deskew & despeckle, Text pass auto-apply
threshold, Run stages automatically, Parallel workers. Footer: primary
"Apply · re-run N pages", "Reset to defaults", and the note "31 approved
pages are left untouched." (PIPE-01 rerun protection).

## 5. Keyboard map

| Key | Action | Where |
| --- | --- | --- |
| J / K | next / previous unresolved issue | Review |
| A | accept selected candidate | Review |
| E | edit current word | Review |
| S | skip (keep original) | Review |
| L | later (defer) | Review |
| 1-9 | choose candidate | Review |
| Cmd/Ctrl+Enter | approve page | Review |
| Ctrl+L | layout mode | Review |
| V / R / X / M | select / draw / split / merge | Layout |
| Esc | close sheet, leave layout mode | everywhere |
| PageUp / PageDown | previous / next page | Review, Layout |
| Ctrl+= / Ctrl+- / Ctrl+0 | zoom in / out / fit | scan pane |
| Ctrl+S | save | everywhere |

All shortcuts are listed on the Shortcuts settings page and are remappable
later. Every action has a visible control equivalent (UX-05).

## 6. Component inventory (Svelte 5)

Shell: `TitleBar`, `StatusBar`, `PipelineRail`, `RailRow`, `ReviewSummary`,
`Inspector`, `InspectorTabs`, `Sheet`, `Toast`.

Pages: `WelcomePage`, `RecentBookCard`, `BookTable`, `DropZone`,
`ProcessingView`, `PageGrid`, `PageTile`, `JobMonitor`, `LogView`,
`SettingsPage`, `SettingsRow`.

Review: `Filmstrip`, `ScanPane`, `ScanToolbar`, `PageCanvas` (bitmap plus
overlay layer in PDF coordinates), `RegionOverlay`, `WordHighlight`,
`TranscriptPane`, `ThresholdSlider`, `Mark`, `IssueInspector`,
`EvidenceCard`, `CandidateList`, `DecisionButtons`, `HistoryList`,
`ApprovePageButton`, `PageInspector`.

Layout: `LayoutToolbar`, `RegionHandles`, `ReadingOrderList`,
`RegionInspector`, `TypeChips`, `BoundsFields`.

Export: `ExportSheet`, `ReadinessCounters`, `ExportScopeRadio`,
`PageStructureChoice`, `ExportProgress`.

Text pass and AI: `TextPassTab`, `AutoApplySlider`, `AiTab`, `ConsentCard`.

Primitives: `Button` (primary / secondary / danger), `Segmented`, `Toggle`,
`Slider`, `Select`, `Checkbox`, `Radio`, `Chip`, `Kbd`, `ProgressBar`,
`StackedBar`, `StatusDot`, `Tooltip`. Built on Bits UI primitives for focus
and ARIA (UX-05); the reading-order list uses svelte-dnd-action with
keyboard reorder handlers tested explicitly, and long lists use TanStack
Virtual's Svelte adapter. Component state uses Svelte 5 runes; shared
workspace state lives in a few `.svelte.ts` stores (project, pipeline,
review selection, settings).

## 7. Open design items

- Bench-only elements (job log, history list, hover readout, page
  mini-grid) are adopted for both themes; Paper gets them in Paper styling.
- Region type chips for catchword, illustration, table, and uncertain are
  not drawn in the mockups; they follow the existing chip style.
- Error and empty states (failed page, save failed, no recent books,
  missing model pack) are not in the mockups and need a pass.
- Accessible names and focus order are to be verified with axe-core once
  the shell exists (UX-05).
