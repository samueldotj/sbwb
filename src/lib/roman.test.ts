import { describe, expect, it } from "vitest";
import { romanInfo, toRoman } from "./roman";

const readings = (t: string) => romanInfo(t)?.readings.map((r) => `${r.text}=${r.value}`);

describe("Roman numerals", () => {
  it("reads numerals as they stand", () => {
    expect(romanInfo("xvii.")).toMatchObject({ core: "xvii", value: 17, standard: null });
    expect(romanInfo("MDCCCXXXIX")?.value).toBe(1839);
    expect(romanInfo("XIV,")?.value).toBe(14);
    expect(romanInfo("(xlix)")?.value).toBe(49);
    expect(romanInfo("I.")?.value).toBe(1);
  });
  it("gives the standard form of old-style and mixed-case numerals", () => {
    expect(romanInfo("CLXXXVIIII")).toMatchObject({ value: 189, standard: "CLXXXIX" });
    expect(romanInfo("iiii")).toMatchObject({ value: 4, standard: "iv" });
    expect(romanInfo("Iv")).toMatchObject({ value: 4, standard: "IV" });
    expect(romanInfo("XXiv")).toMatchObject({ value: 24, standard: "XXIV" });
    expect(romanInfo("Vii")).toMatchObject({ value: 7, standard: "VII" });
    expect(romanInfo("xxXi.")).toMatchObject({ value: 31, standard: "xxxi" });
  });
  it("offers what a misread numeral could have been", () => {
    expect(readings("IL")).toEqual(["II=2", "LI=51"]);
    expect(readings("IIL.")?.[0]).toBe("III=3");
    expect(readings("VIL")?.[0]).toBe("VII=7");
    expect(readings("1V")?.[0]).toBe("IV=4");
    expect(readings("1I")?.[0]).toBe("II=2");
    expect(readings("X1iv")).toContain("xliv=44");
    expect(readings("xIvi")).toContain("xlvi=46");
    expect(readings("Ixvii")).toContain("lxvii=67");
    expect(readings("iij")?.[0]).toBe("iii=3");
    expect(romanInfo("IL")?.value).toBeNull();
  });
  it("leaves words and numbers alone", () => {
    for (const w of ["did", "civil", "ill", "Lux", "cum", "Du", "I", "a", "c.", "the", "1839", "11"]) expect(romanInfo(w)).toBeNull();
    expect(romanInfo("LIV")?.value).toBe(54);
    expect(romanInfo("xi.")?.value).toBe(11);
    expect(romanInfo("Diu,")).toBeNull();
    for (const w of ["Vv", "LL", "vv"]) expect(romanInfo(w)?.value ?? null).toBeNull();
    expect(readings("Il.")?.[0]).toBe("II=2");
  });
  it("writes numbers as numerals", () => {
    expect(toRoman(1839, true)).toBe("MDCCCXXXIX");
    expect(toRoman(49)).toBe("xlix");
  });
});
