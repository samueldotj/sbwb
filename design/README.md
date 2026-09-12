# Design

UI design material for SBWB. The versioned specification derived from the
mockups is [docs/design.md](../docs/design.md); the mockup exports themselves
are kept locally and are **not committed** (D-16, see `.gitignore`).

## sbwb-redesign (local only)

Imported 2026-09-12 from the Claude Design project "Book Scanner Redesign"
(https://claude.ai/design/p/5b452fdb-a0dd-4f9a-b008-c215b82c9c2d) via its
handoff zip. Every "SCBWD" in the export was renamed case-preservingly to
"SBWB" / "sbwb", including the file name and the `.scbwd` project extension.

To restore the folder on another machine: open the project link above, use
"Export" / "Send to Claude Code" to download the handoff zip, unzip
`project/` into `design/sbwb-redesign/`, and apply the same rename.

| File | What it is |
| --- | --- |
| `SBWB Redesign.dc.html` | The design document: one turn, ten 1280x820 artboards. Opens in a browser; needs `support.js` next to it and loads React from a CDN. |
| `support.js` | Claude Design's generated runtime. Not ours; do not edit. |
| `HANDOFF-README.md` | The README shipped in the export. Its instructions are addressed to whoever implements the design; they are not project decisions. |
| `uploads/*.png` | Nine screenshots of the pre-existing "SCBWD Core 0.1.0" app the design brief calls "the current app". Reference only. |

## How the mockups are used

- The mockups take priority over the requirements prose for UI and UX
  (D-15). Requirements were amended to match; `docs/design.md` records the
  layout skeleton, tokens, screens, keyboard map, and component inventory.
- Paper is the light theme and default, Bench the dark theme (D-17).
- Artboard references in the docs use the mockup ids: 1a-1e Paper, 1f-1j
  Bench.
