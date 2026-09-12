# Fixtures

Inputs used for development, evaluation, and acceptance scenarios. Files larger
than ~10 MB are git-ignored; this file records where to get them and their
checksums so a fresh checkout can be rebuilt.

## corpus/hough-1839-vol1

James Hough, *The History of Christianity in India*, Vol. 1 (London, 1839).
Google Books digitization (producer string "Google Books PDF Converter rel 3").
The book text is public domain. Google's front-matter page asks that its scans be
used for personal, non-commercial purposes, so keep the full scan out of public
redistribution until that question is settled.

| File | Pages | SHA-256 | In git |
| --- | --- | --- | --- |
| `source-full.pdf` | 529 | `c91e0c248d4e2a66f3fe329822d34a266423b8ded0353a6175e99d283730667e` | no (15.5 MB) |
| `pages-001-110.pdf` | 110 | `468f50af9a904513ef3595eb86239866bf2384fa33fc66f158b08dc7159773fd` | yes (3.2 MB) |

`pages-001-110.pdf` was produced with pypdf by copying page objects 1-110
unchanged; image streams are byte-identical to the full scan. It is the trial
fixture referenced by the requirements (NFR-02 "10-page trial" can use pages
1-10 of it; the "100-page mixed-layout book" is pages 1-110).

Scan characteristics (matter for pipeline design):

- Page media box about 355 x 606 pt (roughly 4.9 x 8.4 in).
- One JBIG2 bilevel image per page, about 3000 x 5050 px, so roughly 600 DPI.
  Rendering at the nominal 300 DPI downsamples; region OCR may want native
  resolution for small marginalia.
- A second small image per page (`/Wm`, 1034 x 204) is the Google watermark.
- Google's existing OCR text layer is present but poor (missing word spaces).
  It can serve as a baseline for CER/WER comparison, not as ground truth.
- Layout: running heads alternating verso/recto ("HISTORY OF CHRISTIANITY." /
  "IN INDIA: BOOK I."), printed page numbers in the head line, chapter and
  date side notes in the outer margin ("CHAP. I.", "A.D. 1566."), footnotes.
- Language profile: 19th-century British English, not Early Modern English.

## lexicons

| File | Source | License | Use |
| --- | --- | --- | --- |
| `webster-1913-gutenberg-29765.txt` (29 MB, not in git) | Project Gutenberg eBook #29765, https://www.gutenberg.org/ebooks/29765 | Public domain (Gutenberg license for the file) | Period vocabulary for 19th-century texts; headword extraction |
| `en_GB.dic`, `en_GB.aff`, `en_GB-README.txt` | LibreOffice dictionaries repo, `en/` | LGPL 3 / see README | Modern British spelling, Hunspell format |
| `hunspell-en_GB-large/` | SCOWL release rel-2026.02.25, `hunspell-en_GB-large-2026.02.25.zip` from GitHub en-wl/wordlist releases, unpacked | SCOWL permissive (MIT-like, see README) | Larger British list incl. variants |
| `scowl-2026.02.25.tar.gz` (not in git) | GitHub en-wl/wordlist tag rel-2026.02.25 | SCOWL permissive | Build custom size-95 lists incl. archaic words |

Not yet fetched, listed for later evaluation:

- EEBO-TCP Phase I texts (GitHub textcreationpartnership/Texts, CC0) for an
  Early Modern English frequency list; multi-GB.
- Google Books Ngram 1-grams (CC BY 3.0) filtered to 1750-1900 for period
  word frequencies; tens of GB uncompressed.
- MorphAdorner Early Modern lexicon (NCSA license) for spelling-variant tables.
