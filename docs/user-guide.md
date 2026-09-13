# SBWB user guide

SBWB (Scanned Book Work Bench) turns a scanned book PDF into an editable
Word document whose every word can be traced back to the scan. Everything
runs on your computer; nothing is uploaded.

## 1. Workflow

1. **Import** a PDF (Welcome page, File menu, or drop it on the window).
   Choose the page scope; the default trial is the first 50 pages. Processing
   starts on its own: OCR, layout analysis, and the text pass run page by page
   and can be paused from the rail.
2. **Review.** Open a page from the grid or the filmstrip. The transcript
   shows the effective text as paragraphs; amber marks are open issues, green
   marks are accepted or automatic changes. The Issue tab shows the scan
   evidence, the current reading, numbered candidates, and the decision
   buttons. J and K move through the issues of the whole book.
3. **Approve** a page when you are done with it (Ctrl+Enter). Unresolved
   issues can be acknowledged; the approval records exactly what was covered
   and turns "outdated" if the page changes later.
4. **Export** to Word (Ctrl+E). A working copy is always available; a clean
   copy needs every page in scope approved.

## 2. Keyboard shortcuts

| Key | Action |
| --- | --- |
| J / K | Next / previous issue across the book |
| A / E / S / L | Accept the chosen candidate / Edit the word / Skip (keep the original) / Later (defer) |
| 1–9 | Choose a candidate |
| R | Recognise a "missing text" area at 300 dpi ×3 |
| Ctrl+Enter | Approve the page (or remove the approval) |
| Ctrl+L | Layout mode; V / R / X / M select, draw, split, merge; Esc returns |
| PgUp / PgDn | Previous / next page |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / fit |
| Ctrl+E | Export to Word |
| Ctrl+I / Ctrl+O / Ctrl+W | Import / Open / Close |
| Ctrl+, | Settings |

## 3. What the marks and scores mean

- Scores are heuristics from 0 to 100, labelled with their source ("64% ·
  OCR", "92 · text pass"). They are not probabilities. The second engine
  (ocrs) gives line-level readings and is always shown as unscored.
- The review threshold (default 70) shows scored issues strictly below it.
  Issues without a score (missing text, clipping, uncertain order, flags,
  second-engine disagreements) stay in the inbox at every threshold.
- Automatic changes made by the text pass (hyphen joins, single-character OCR
  confusions at 90 and above) keep their source words and can be undone from
  the Issue tab history.

## 4. Page furniture on export

The Layout mode reading-order list shows where each region goes.

| Policy | Running heads | Page numbers | Side notes | Footers | Footnotes |
| --- | --- | --- | --- | --- | --- |
| Styled paragraphs (default) | "Running head" style at the top of the page's text | "Printed page number" style | "Side note" style before the paragraph they annotate | "Footer note" style after the page | "Footnote text" style after the body |
| Native headers and footers | Word page header | Word page header | Word page footer | Word page footer | "Footnote text" paragraphs |

"Mirror the scan" puts a page break (or a next-page section break under the
native policy) after each source page. "Continuous text" has no breaks and
omits running heads; the sheet states this before you export. A word joined
across a page boundary belongs to the earlier page and is never duplicated.

## 5. Working copy versus clean copy

The working copy highlights every unresolved word below the flag threshold
and attaches one Word comment per flag. The clean copy has no annotations
and is only offered when every page in scope is currently approved. Every
export writes a report next to the document and, optionally, an archive
bundle with PAGE XML and a JSON transcript with the source map.

## 6. AI proofreading (optional)

The AI tab in the inspector is off until you enable it. Add a developer API
key for OpenAI, Anthropic, or Google (consumer subscriptions do not include
API access); keys are kept for the session unless you tick "remember",
which stores them in the Windows credential store. Pick a model from the
discovered list or type a model id. Choose the pages, read the "What leaves
this machine" card (the saved words of those pages with ids and the page
numbers; never the PDF, images, paths, or metadata), tick the consent box,
and send. Each page is one request with no retries; a budget caps requests
and page size; cost is shown as unknown. Suggestions arrive in the review
inbox labelled with the provider and model, unscored, and are never applied
automatically.

## 7. Book queue

"Queue several PDFs…" on the Welcome page (or File › Book queue) takes
several PDFs with a processing profile and a destination folder. Each PDF
becomes its own project and is processed in order while you keep working;
pending books can be reordered or removed (the project is never deleted),
a failed book does not stop the others, and a queue interrupted by closing
SBWB is only resumed when you press Start again.

## 8. Early Modern English

Settings › Processing › Language profile switches the text pass to an Early
Modern profile in which period spellings (haue, vnto, iudge, warre, citie)
count as known words and are never proposed for correction. Changing the
profile reruns only the text pass.

## 9. Limitations in this release

- Windows 11 x64 only. macOS and Linux builds compile but are not qualified.
- English (British) lexicons; the Early Modern profile is rule-based, not a period dictionary.
- Tables are exported as plain text, an image, or skipped, never as Word
  table cells. Footnotes are kept as labelled paragraphs, not Word footnotes.
- Illustrations are exported at 150 dpi crops.
- AI proofreading needs a provider key and a network connection; it never runs on its own.
- Word pagination is not checked; source page boundaries are preserved as
  breaks, not as identical physical pages.

## 10. Where things are

- Project file: `<book>.sbwb` next to the PDF (or where you chose), with a
  `<book>.sbwb.cache` folder of renders that can be cleared from Settings.
- Logs: `%LOCALAPPDATA%\io.github.samueldotj.sbwb\logs` with home paths
  redacted.
- Exports: `<book>.docx` beside the project file by default (an earlier export
  there is replaced; pick another destination from the sheet), with
  `<name>.export-report.json` and optionally `<name>.sbwb-archive.zip`. The
  Word output list in the rail opens them.
