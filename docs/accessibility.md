# Accessibility audit (UX-05, M9.7)

Automated audit with axe-core 4.10.2 on the review workspace
(`/?mock=review` on the Vite dev server, which renders the same components
as the shell), rules: WCAG 2.0/2.1/2.2 A and AA plus best practices.
Run on 2026-09-12 against commit `a0d6c92` plus the fixes below.

## Findings and fixes

| Finding | Theme | Fix |
| --- | --- | --- |
| `color-contrast`: muted text (`--muted #8a8177`) at 2.3–3.6:1 on panel, chrome, and paper backgrounds (28 nodes) | Paper | `--muted` darkened to `#6a6157` (4.8:1 on chrome, 5.5:1 on paper); `--accent-text` darkened to `#7a4210`; the rail's running value uses `--accent-text`; the scan placeholder uses `--text` |
| `page-has-heading-one` | both | The book title in the title bar is an `h1` |
| `target-size`: rail filter checkboxes 13 px with 3 px spacing | both | Filter rows have a 24 px minimum height, so the spacing exception applies |
| `target-size`: page mini-grid cells 12.9 px (110 nodes) | Bench | Left as designed: the 10-column mini-grid is a map, and equivalent page navigation exists on the same screen (filmstrip on Paper, PgUp/PgDn, the Go-to field, J/K), which is the WCAG 2.5.8 "equivalent" exception. Recorded here rather than hidden. |

Result after the fixes: Paper 0 violations, 47 rules passed, 2 rules
incomplete (contrast on gradient/overlay elements and target size on
elements axe could not measure); Bench 0 violations other than the
mini-grid exception above, 44 rules passed.

## Keyboard and screen-reader behaviour (manual)

- Every action has a visible control and a documented shortcut
  (`docs/user-guide.md` section 2; Settings › Shortcuts).
- Focus stays in the inspector after a decision; the next issue is announced
  through a polite live region ("Issue 4 of 12 on page 48: …").
- Marks in the transcript are buttons with names of the form
  "word: uncertain reading"; the source word on the scan highlights on
  focus as well as on hover.
- Dialogs (Export, grouped correction, the close prompt) trap focus, name
  themselves, and close on Esc.
- Large text (Settings › Appearance) scales the interface and the transcript.

Pending (release qualification): a walkthrough of import, review, approve,
and export with Windows Narrator or NVDA. The live-region wording and the
focus order are designed for it but have not been verified by ear.
