// Character highlights in the transcript: digits, ordinary punctuation, and
// characters that are neither letters, digits nor ordinary punctuation (OCR
// noise like ¢ ® * |).

export type HighlightKind = "num" | "punct" | "sym";
export type HighlightOpts = { numbers: boolean; punctuation: boolean; special: boolean };
export type Run = { text: string; kind: HighlightKind | null };

/** Punctuation a printed book is expected to have. */
const ORDINARY = new Set([...".,;:!?'\"‘’“”()-–—"]);
const NUMBER = /\p{N}/u;
const PLAIN = /[\p{L}\p{M}\s]/u;

export function charKind(ch: string): HighlightKind | null {
  if (NUMBER.test(ch)) return "num";
  if (ORDINARY.has(ch)) return "punct";
  if (PLAIN.test(ch)) return null;
  return "sym";
}

const WANTED: Record<HighlightKind, keyof HighlightOpts> = { num: "numbers", punct: "punctuation", sym: "special" };

/** Split text into runs, marking the characters the options ask for. */
export function charRuns(text: string, opts: HighlightOpts): Run[] {
  const out: Run[] = [];
  for (const ch of text) {
    let kind = charKind(ch);
    if (kind && !opts[WANTED[kind]]) kind = null;
    const last = out[out.length - 1];
    if (last && last.kind === kind) last.text += ch;
    else out.push({ text: ch, kind });
  }
  return out;
}

/** How many characters of each kind the texts hold. */
export function countKinds(texts: Iterable<string>): Record<HighlightKind, number> {
  const n = { num: 0, punct: 0, sym: 0 };
  for (const t of texts) {
    for (const ch of t) {
      const k = charKind(ch);
      if (k) n[k]++;
    }
  }
  return n;
}
