// Roman numerals in the transcript: read a word as a numeral, and when it
// looks like a misread one (OCR: 1 or l for i, I for l, j for a final i,
// mixed case) offer the numerals it could have been.

export type RomanReading = { text: string; value: number };
export type RomanInfo = {
  /** The word without surrounding punctuation. */
  core: string;
  /** Its value when the word is a numeral as it stands. */
  value: number | null;
  /** The standard spelling, when the word is an old-style or mixed-case form (iiii, CLXXXVIIII, Iv). */
  standard: string | null;
  /** Numerals a misread word could have been, best first. */
  readings: RomanReading[];
};

const VALUES: Record<string, number> = { i: 1, v: 5, x: 10, l: 50, c: 100, d: 500, m: 1000 };
/** A well-formed numeral, lower case. */
const STANDARD = /^(?=[ivxlcdm])m{0,4}(c[md]|d?c{0,3})(x[cl]|l?x{0,3})(i[xv]|v?i{0,3})$/;
/** Words spelt only with numeral letters that are words, not numerals,
 *  when written in lower case. */
const WORDS = new Set(
  "did civil civic ill illi illic illud mild mill dill vivid livid mimic lid mid dim mix vim dic dici lux cum im imi id il liv llc mm jul cui vic ic lui ci mi di li dix".split(" "),
);
/** ... and those of them also written with a capital (Lux, Liv., Id.). */
const TITLE_WORDS = new Set("did civil civic lux cum liv id jul vic ic di mi li dix illi lui cui mild mill vivid livid dim mix".split(" "));
/** What each character may have been in a numeral, likeliest first. */
const CONFUSIONS: Record<string, string[]> = {
  "1": ["i", "l"],
  "|": ["i"],
  "!": ["i"],
  j: ["i"],
  l: ["l", "i"],
  i: ["i", "l"],
};
const LIKELY = /^[ivxlcdm1|!j]+$/;

function value(lower: string): number | null {
  if (!STANDARD.test(lower)) return null;
  let n = 0;
  for (let k = 0; k < lower.length; k++) {
    const a = VALUES[lower[k]!]!;
    const b = VALUES[lower[k + 1] ?? ""] ?? 0;
    n += a < b ? -a : a;
  }
  return n;
}

/** Old-style numerals: additive, a letter repeated up to five times
 *  (iiii, viiii, CLXXXVIIII), with the usual subtractive pairs. */
function oldStyleValue(lower: string): number | null {
  // v, l and d are never repeated
  if (!/^[ivxlcdm]+$/.test(lower) || /(.)\1{5}/.test(lower) || /v.*v|l.*l|d.*d/.test(lower)) return null;
  let n = 0;
  let last = Infinity;
  for (let k = 0; k < lower.length; k++) {
    const a = VALUES[lower[k]!]!;
    const b = VALUES[lower[k + 1] ?? ""] ?? 0;
    if (a < b) {
      if (!["iv", "ix", "xl", "xc", "cd", "cm"].includes(lower[k]! + lower[k + 1]!) || b - a > last) return null;
      n += b - a;
      // what follows "iv" must be smaller than i, after "xc" smaller than x
      last = a - 1;
      k++;
    } else {
      if (a > last) return null;
      n += a;
      last = a;
    }
  }
  return n;
}

export function toRoman(n: number, upper = false): string {
  const table: [number, string][] = [[1000, "m"], [900, "cm"], [500, "d"], [400, "cd"], [100, "c"], [90, "xc"], [50, "l"], [40, "xl"], [10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"]];
  let out = "";
  for (const [v, s] of table) {
    while (n >= v) {
      out += s;
      n -= v;
    }
  }
  return upper ? out.toUpperCase() : out;
}

/** Read a transcript word as a Roman numeral, or null when it does not look like one. */
export function romanInfo(token: string): RomanInfo | null {
  const core = token.replace(/^[^\p{L}\p{N}|!]+|[^\p{L}\p{N}|!]+$/gu, "");
  const lower = core.toLowerCase();
  if (!core || !LIKELY.test(lower) || !/[ivxlcdm]/.test(lower) || core.length > 12) return null;
  const letters = core.replace(/[^\p{L}]/gu, "");
  const title = letters.length > 1 && letters[0] === letters[0]!.toUpperCase() && letters.slice(1) === letters.slice(1).toLowerCase();
  if ((letters === letters.toLowerCase() && WORDS.has(lower)) || (title && TITLE_WORDS.has(lower))) return null;
  // single letters: v and x, or i with a full stop (Vol. I.)
  if (core.length === 1 && !/[vx]/i.test(core) && !(/i/i.test(core) && /\.$/.test(token))) return null;
  const upper = letters.length > 0 && letters.replace(/\P{Lu}/gu, "").length * 2 >= letters.length;
  const cased = (s: string) => (upper ? s.toUpperCase() : s);

  const v = value(lower);
  if (v !== null) {
    const uniform = letters === letters.toUpperCase() || letters === letters.toLowerCase();
    // mixed case follows the first letter: Vii, Xi -> VII, XI
    const first = letters[0] === letters[0]?.toUpperCase() ? lower.toUpperCase() : lower;
    return { core, value: v, standard: uniform ? null : first, readings: [] };
  }
  const old = oldStyleValue(lower);
  if (old !== null) return { core, value: old, standard: toRoman(old, upper), readings: [] };

  // a misread numeral: try what each character may have been
  let options: { text: string; cost: number }[] = [{ text: "", cost: 0 }];
  for (const ch of lower) {
    const alts = CONFUSIONS[ch] ?? [ch];
    options = options.flatMap((o) => alts.map((a, k) => ({ text: o.text + a, cost: o.cost + (a === ch ? 0 : 1) + k * 0.1 })));
    if (options.length > 512) return null;
  }
  const seen = new Set<string>();
  const readings = options
    .filter((o) => o.text !== lower && value(o.text) !== null)
    .sort((a, b) => a.cost - b.cost)
    .filter((o) => !seen.has(o.text) && seen.add(o.text))
    .slice(0, 3)
    .map((o) => ({ text: cased(o.text), value: value(o.text)! }));
  return readings.length ? { core, value: null, standard: null, readings } : null;
}
