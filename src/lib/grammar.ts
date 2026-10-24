// Simple grammar and typography checks on a page's words: spacing,
// punctuation, quotation marks, capitals after a full stop, repeated words.
// They only point things out; a fix is an ordinary text edit of the words
// it names.

export type GrammarWord = { id: string; text: string; trailing: string };
export type GrammarRule = "spaces" | "after_punct" | "before_punct" | "quote" | "capital" | "repeated";
export type GrammarFix = { spans: string[]; text: string };
export type GrammarFlag = { rule: GrammarRule; message: string; fix: GrammarFix | null };

/** Lower-case abbreviations a full stop does not end a sentence after. */
const ABBREVIATIONS = new Set(
  "etc viz vol vols chap ch sect ibid seq edit ed trans ult inst cf art lib cap fol ff pp p mr mrs dr st rev esq ver vide sc".split(" "),
);
/** A well-formed lower-case Roman numeral (iii, xvii, xlix). */
const ROMAN = /^(?=[ivxlcdm])m*(c[md]|d?c{0,3})(x[cl]|l?x{0,3})(i[xv]|v?i{0,3})$/;
const LONE_PUNCT =/^[,.;:!?]+$/;
/** Trailing closing quotes and brackets, which come after a full stop. */
const CLOSERS = /[”’"')\]]+$/u;
const OPENERS = /^[“‘"'([]+/u;

/** The word without surrounding punctuation, lower-cased. */
function core(t: string): string {
  return t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "").toLowerCase();
}

/** Whether a word closes a sentence (and is not an abbreviation). */
function endsSentence(t: string): boolean {
  const s = t.replace(CLOSERS, "");
  if (!/[.!?]$/.test(s)) return false;
  if (/[!?]$/.test(s)) return true;
  const word = s.slice(0, -1);
  // initials and short abbreviations (J. pp. vol.) and dotted ones (i.e. A.D.)
  if (!/\p{L}{3}$/u.test(word) || word.includes(".")) return false;
  // lower-case Roman numerals (vol. iii. c. 31)
  const c = core(word);
  return !ABBREVIATIONS.has(c) && !ROMAN.test(word.replace(OPENERS, ""));
}

/** Upper-case the first letter after any opening quotes or brackets. */
function capitalise(t: string): string {
  const lead = t.match(OPENERS)?.[0] ?? "";
  const rest = t.slice(lead.length);
  return lead + rest.charAt(0).toUpperCase() + rest.slice(1);
}

/** Check a page's words in reading order. Empty words (removed by an edit)
 *  are skipped. Returns the flags found on each word id. */
export function checkGrammar(words: GrammarWord[]): Map<string, GrammarFlag[]> {
  const out = new Map<string, GrammarFlag[]>();
  const add = (id: string, f: GrammarFlag) => {
    const list = out.get(id);
    if (list) list.push(f);
    else out.set(id, [f]);
  };
  const ws = words.filter((w) => w.text);
  ws.forEach((w, i) => {
    const prev = ws[i - 1];
    const next = ws[i + 1];
    const t = w.text;
    const one = (text: string): GrammarFix => ({ spans: [w.id], text });

    if (/ {2,}|\t/.test(w.trailing) || /\s/.test(t)) {
      add(w.id, { rule: "spaces", message: "More than one space between words", fix: null });
    }
    if (/\p{Ll}[,;:]\p{L}/u.test(t)) {
      add(w.id, { rule: "after_punct", message: "No space after the comma or semicolon", fix: one(t.replace(/(\p{Ll}[,;:])(?=\p{L})/gu, "$1 ")) });
    }
    if (/\p{Ll}{2}[.!?]\p{Lu}\p{Ll}/u.test(t)) {
      add(w.id, { rule: "after_punct", message: "No space after the full stop", fix: one(t.replace(/(\p{Ll}{2}[.!?])(?=\p{Lu}\p{Ll})/gu, "$1 ")) });
    }
    if (prev && LONE_PUNCT.test(t)) {
      add(w.id, { rule: "before_punct", message: `Space before “${t}”`, fix: { spans: [prev.id, w.id], text: prev.text + t } });
    }
    // quotation marks
    if (/[\p{L}\p{N}.,;:!?][“‘]/u.test(t)) {
      add(w.id, { rule: "quote", message: "No space before the opening quotation mark", fix: one(t.replace(/([\p{L}\p{N}.,;:!?])([“‘])/gu, "$1 $2")) });
    }
    if (/”[\p{L}\p{N}]/u.test(t)) {
      add(w.id, { rule: "quote", message: "No space after the closing quotation mark", fix: one(t.replace(/”(?=[\p{L}\p{N}])/gu, "” ")) });
    }
    if (/\p{L}"\p{L}/u.test(t)) {
      add(w.id, { rule: "quote", message: "No space around the quotation mark", fix: null });
    }
    if (prev && /^”/.test(t)) {
      add(w.id, { rule: "quote", message: "Space before the closing quotation mark", fix: { spans: [prev.id, w.id], text: prev.text + t } });
    }
    if (next && /“$/.test(t)) {
      add(w.id, { rule: "quote", message: "Space after the opening quotation mark", fix: { spans: [w.id, next.id], text: t + next.text } });
    }
    // a capital after a full stop
    if (prev && endsSentence(prev.text) && /^\p{Ll}/u.test(t.replace(OPENERS, ""))) {
      add(w.id, { rule: "capital", message: "Capital letter expected after the full stop", fix: one(capitalise(t)) });
    }
    // the same word twice
    const c = core(t);
    if (prev && c.length >= 2 && /\p{L}/u.test(c) && c === core(prev.text) && !/[^\p{L}\p{N}]$/u.test(prev.text) && !/-$/.test(t)) {
      add(w.id, { rule: "repeated", message: `“${prev.text}” twice`, fix: { spans: [prev.id, w.id], text: prev.text } });
    }
  });
  return out;
}
