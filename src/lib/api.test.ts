import { describe, expect, it } from "vitest";
import { formatBytes, parseRanges, scopeLabel } from "./api";

describe("parseRanges", () => {
  it("parses ranges and singles", () => {
    expect(parseRanges("1-50, 60–70; 80")).toEqual([
      [1, 50],
      [60, 70],
      [80, 80],
    ]);
  });
  it("rejects garbage and inverted ranges", () => {
    expect(parseRanges("abc")).toBeNull();
    expect(parseRanges("50-1")).toBeNull();
    expect(parseRanges("0-3")).toBeNull();
    expect(parseRanges("")).toBeNull();
  });
});

describe("labels", () => {
  it("formats scope and sizes", () => {
    expect(scopeLabel([[1, 50], [60, 60]])).toBe("1–50, 60");
    expect(formatBytes(15_534_508)).toBe("15 MB");
  });
});
