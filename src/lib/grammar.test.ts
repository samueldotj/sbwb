import { describe, expect, it } from "vitest";
import { checkGrammar, type GrammarWord } from "./grammar";

/** Words from a sentence, one space after each (ids are w0, w1, ...). */
const words = (s: string): GrammarWord[] => s.split(" ").map((text, i) => ({ id: `w${i}`, text, trailing: " " }));
const rules = (s: string) => [...checkGrammar(words(s))].map(([id, fs]) => `${id}:${fs.map((f) => f.rule).join("+")}`);

describe("grammar checks", () => {
  it("finds nothing in clean text", () => {
    expect(rules("He said, “It is so.” Then he left; the rest stayed. Pp. 12, vol. iii. c. 31, i.e. not so.")).toEqual([]);
  });
  it("wants a capital after a full stop, not after abbreviations", () => {
    const g = checkGrammar(words("of the land. and is"));
    expect(g.get("w3")?.[0]).toMatchObject({ rule: "capital", fix: { spans: ["w3"], text: "And" } });
    expect(rules("see vol. ii. and etc. more, Sect. iv. and xvii. of")).toEqual([]);
    expect(rules("he wrote. “the end")).toEqual(["w2:capital"]);
    expect(rules("was civil. and")).toEqual(["w2:capital"]);
    expect(checkGrammar(words("he wrote. “the end")).get("w2")?.[0]?.fix?.text).toBe("“The");
  });
  it("finds missing and extra spaces around punctuation", () => {
    const g = checkGrammar(words("dignity,or powers ; and end.The"));
    expect(g.get("w0")?.[0]).toMatchObject({ rule: "after_punct", fix: { text: "dignity, or" } });
    expect(g.get("w2")?.[0]).toMatchObject({ rule: "before_punct", fix: { spans: ["w1", "w2"], text: "powers;" } });
    expect(g.get("w4")?.[0]).toMatchObject({ rule: "after_punct", fix: { text: "end. The" } });
  });
  it("checks quotation marks", () => {
    expect(checkGrammar(words("said“Go"))?.get("w0")?.[0]?.fix?.text).toBe("said “Go");
    expect(checkGrammar(words("went.”Then"))?.get("w0")?.[0]?.fix?.text).toBe("went.” Then");
    expect(checkGrammar(words("word ” next")).get("w1")?.[0]).toMatchObject({ rule: "quote", fix: { spans: ["w0", "w1"], text: "word”" } });
    expect(checkGrammar(words("“ Go")).get("w0")?.[0]).toMatchObject({ rule: "quote", fix: { spans: ["w0", "w1"], text: "“Go" } });
    expect(rules('the"word')).toEqual(["w0:quote"]);
  });
  it("finds repeated words and wide gaps", () => {
    const g = checkGrammar(words("of the the sect"));
    expect(g.get("w2")?.[0]).toMatchObject({ rule: "repeated", fix: { spans: ["w1", "w2"], text: "the" } });
    expect(rules("in In- fact")).toEqual([]);
    expect(rules("the, the")).toEqual([]);
    const wide: GrammarWord[] = [{ id: "a", text: "two", trailing: "  " }, { id: "b", text: "spaces", trailing: " " }];
    expect(checkGrammar(wide).get("a")?.[0]?.rule).toBe("spaces");
  });
  it("skips words an edit removed", () => {
    const ws: GrammarWord[] = [{ id: "a", text: "the", trailing: " " }, { id: "x", text: "", trailing: "" }, { id: "b", text: "the", trailing: " " }];
    expect(checkGrammar(ws).get("b")?.[0]?.rule).toBe("repeated");
  });
});
