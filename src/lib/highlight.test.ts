import { describe, expect, it } from "vitest";
import { charKind, charRuns, countKinds } from "./highlight";

describe("character highlights", () => {
  it("classifies digits, punctuation, OCR noise and letters", () => {
    expect(charKind("7")).toBe("num");
    expect(charKind("³")).toBe("num");
    for (const ch of [..."¢®*|{§~#"]) expect(charKind(ch)).toBe("sym");
    for (const ch of [...".,;:!?'’“”()-—"]) expect(charKind(ch)).toBe("punct");
    for (const ch of [..."aÉæ "]) expect(charKind(ch)).toBeNull();
  });
  it("splits a word into runs for the chosen options", () => {
    const all = { numbers: true, punctuation: true, special: true };
    expect(charRuns("Mar®'", all)).toEqual([
      { text: "Mar", kind: null },
      { text: "®", kind: "sym" },
      { text: "'", kind: "punct" },
    ]);
    expect(charRuns("pp.12—14", { numbers: true, punctuation: false, special: true })).toEqual([
      { text: "pp.", kind: null },
      { text: "12", kind: "num" },
      { text: "—", kind: null },
      { text: "14", kind: "num" },
    ]);
    expect(charRuns("a1*.", { numbers: false, punctuation: false, special: true })).toEqual([
      { text: "a1", kind: null },
      { text: "*", kind: "sym" },
      { text: ".", kind: null },
    ]);
  });
  it("counts each kind", () => {
    expect(countKinds(["A.D.", "610.", "¢", "Sce*"])).toEqual({ num: 3, punct: 3, sym: 2 });
  });
});
