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
   Edit (E) starts from the chosen candidate, so a near miss can be fixed and
   accepted in one step. To correct text that has no issue, double-click a
   word in the transcript to edit it, or select several words and choose
   **Edit text**: what you type replaces the selection (Enter saves, Esc
   cancels), and Undo appears above the transcript. Each typed word stays tied to the scan word it replaces, and
   the edit is re-applied if the text pass runs on the page again.
3. **Approve** a page when you are done with it (Ctrl+Enter). Unresolved
   issues can be acknowledged; the approval records exactly what was covered
   and turns "outdated" if the page changes later.
4. **Export** to Word (Ctrl+E). A working copy is always available; a clean
   copy needs every page in scope approved.

## 2. Keyboard shortcuts

| Key | Action |
| --- | --- |
| J / K | Next / previous issue across the book |
| A / E / S / L | Accept the chosen candidate / Edit and accept / Skip (keep the original) / Later (defer) |
| 1–9 | Choose a candidate |
| R | Recognise a "missing text" area at 300 dpi ×3 |
| Ctrl+Enter | Approve the page (or remove the approval) |
| Ctrl+Z | Undo the last change — a decision, text edit, grouped correction, or approval — going further back each time it is pressed; it opens the page the change was on. A change that a later edit depends on is not undone |
| Ctrl+L | Layout mode; V / R / X / M select, draw, split, merge; in Select, Ctrl- or Shift-click adds a region to the selection and dragging on the page selects every region the rectangle touches; Del deletes the selected regions and the Region tab changes their type together; Ctrl+Z undoes; Esc returns. Layout edits save automatically, and leaving Layout mode re-runs the text pass for every page you edited |
| PgUp / PgDn | Previous / next page |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / fit |
| Ctrl+E | Export to Word |
| Ctrl+I / Ctrl+O / Ctrl+W | Import / Open / Close |
| Ctrl+, | Settings |

## 3. What the marks and scores mean

- **Marginal notes** appear in a column to the left or right of the text,
  on the side their box is on the scan, beside the paragraph they sit
  next to, at its top, middle or bottom. In a narrow transcript pane they
  follow that paragraph. Drag a note to move it: while dragging the text
  is 2pt smaller so more of the page is in view, and each paragraph shows
  Top, Middle and Bottom spots in both margins; drop the note on one. The
  place is saved with the page's layout (the text pass is not re-run).
  The Word export puts a note's box at the start of the paragraph for Top,
  and at the start of the next paragraph for Middle and Bottom.
  Whole-page OCR often skips small side notes. A marginal box with no
  words in it is read again on its own after layout analysis, and when you
  draw one or open its page in Layout mode. The text pass then runs the
  page again.
- **Spelling.** In the transcript's Highlight menu, choose Spelling: UK
  English or US English. Words that are in neither that dictionary, the
  Webster 1913 word list, nor this book's own words get a wavy red
  underline. Click (or right-click) one for suggestions (choosing one is
  an undoable edit) or to add it to this book's words. An added word also
  counts as known to the text pass. Both dictionaries are bundled; nothing
  goes online.
- **Grammar.** Tick Grammar in the Highlight menu to underline simple
  problems in blue: more than one space between words, no space after a
  comma, semicolon or full stop, a space before punctuation (" ;"), a
  missing or extra space at a quotation mark, a lower-case word after a
  full stop (not after abbreviations such as vol., pp., etc. or Roman
  numerals), and a word repeated ("the the"). Click an underlined word
  to see the problem and, where there is one, a **Fix** button that makes
  the change as an undoable edit. Old books often set a space before ; and
  :, so expect many of those.
- **Word menu.** Click any plain word in the transcript for a menu: **Link
  to** lists the footnotes on the page by their first characters (choosing
  one links the note to this word, moving it if it was linked elsewhere),
  spelling suggestions when the word is underlined, and **Edit text**.
  Double-click still opens the editor straight away.
- **Paragraphs.** The word menu also has **¶ New paragraph here**, which
  starts a paragraph at that word, and on the first word of a paragraph
  **Join with paragraph above**. Paragraphs join only within one layout
  region (the first word of a region always starts one). Both can be
  undone (Ctrl+Z), carry into the Word export, and are kept when the text
  pass re-runs the page.
- **Footnotes of several paragraphs.** Each paragraph at the foot of the
  page starts as a footnote of its own. When one is really more of the
  footnote above (no marker of its own), click its first word and choose
  **Continue the footnote above**: it is labelled "footnote · continued",
  leaves the Link to list, and goes into the same Word footnote as the
  note above. **Make a separate footnote** undoes it. This works across
  layout boxes and is kept when the text pass re-runs the page.
- **Roman numerals.** When the word is a Roman numeral, the word menu
  shows its value with **Convert → 17** to write it as a number, and the
  standard form of an old-style or mixed-case one (CLXXXVIIII, Vii). When
  it looks like a misread numeral (1 or l for i, I for l, as in "1V.",
  "VIL", "Ixvii."), it lists the numerals it could have been: **Use**
  corrects the numeral, **Convert** corrects it and writes the number.
  Words such as "did", "civil" or "Du" are left alone. Right-click opens
  the menu on a numeral that has an issue mark.
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
| Native headers and footers (default) | Word page header | Word page header | Bordered text box beside the paragraph they annotate | Word page footer | Word page footer, above the footer line |
| Styled paragraphs | "Running head" style at the top of the page's text | "Printed page number" style | Bordered text box beside the paragraph they annotate | "Footer note" style after the page | "Footnote text" style after the body |

**Footnote references.** The Text pass tab lists the footnotes on the page.
A footnote linked to a word in the text exports as a real Word footnote:
the reference follows that word, and the note moves into Word's footnote
area under either policy. Numbers are automatic: 1, 2, 3 in the order of
the linked words, restarting on each page. **Find printed markers**
suggests links where a word ends with the mark a note starts with (*, †, a
digit, or the "!" and "?" OCR often makes of a dagger); accept or reject
each. To link by hand, select a word in the transcript and choose **Link
to selected** beside the note. The printed marks are removed from both the
word and the note in the export. Links follow their words when the text
pass runs again.

Side-note boxes float on the side of the page the note was printed on and
the body text wraps around them; in Word they are text frames, so they can
be moved or resized like a text box.

"Mirror the scan" puts a page break (or a next-page section break under the
native policy) after each source page. "Continuous text" has no breaks and
omits running heads; the sheet states this before you export. A word joined
across a page boundary belongs to the earlier page and is never duplicated.

## 5. Working copy versus clean copy

The working copy highlights every unresolved word below the flag threshold
and attaches one Word comment per flag; untick "Add a Word comment to each
flagged word" to keep the highlights without comments. The clean copy has no annotations
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

## 9. Updates

**Settings → Updates → Check for updates** asks GitHub for the latest
release. When a newer one exists, **Download and install** fetches its
installer, checks it against the release's `SHA256SUMS.txt`, closes SBWB
(processing stops at the current page; finished work is kept), runs the
installer and reopens SBWB. **At startup** chooses what happens when SBWB
opens: **Off** (the default) never checks; **Notify** shows a new version
in the status bar and downloads nothing without confirmation; **Install**
downloads and verifies it, then installs straight away if no book is open,
or waits in the status bar (**ready · install**) until you click it.
GitHub sees your IP address and the SBWB version, nothing about your books.

## 10. Limitations in this release

- Windows 11 x64 only. macOS and Linux builds compile but are not qualified.
- English (British) lexicons; the Early Modern profile is rule-based, not a period dictionary.
- Tables are exported as plain text, an image, or skipped, never as Word
  table cells. Footnotes are kept as labelled paragraphs, not Word footnotes.
- Illustrations are exported at 150 dpi crops.
- AI proofreading needs a provider key and a network connection; it never runs on its own.
- Word pagination is not checked; source page boundaries are preserved as
  breaks, not as identical physical pages.

## 11. Where things are

- Project file: `<book>.sbwb` next to the PDF (or where you chose), with a
  `<book>.sbwb.cache` folder of renders that can be cleared from Settings.
- Logs: `%LOCALAPPDATA%\io.github.samueldotj.sbwb\logs` with home paths
  redacted.
- Exports: `<book>.docx` beside the project file by default (an earlier export
  there is replaced; pick another destination from the sheet), with
  `<name>.export-report.json` and optionally `<name>.sbwb-archive.zip`. The
  Word output list in the rail opens them.
