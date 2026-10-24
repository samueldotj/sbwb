import { describe, expect, it } from "vitest";
import { formatBytes, latestUndoable, parseRanges, scopeLabel, type HistoryEntry } from "./api";

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

describe("latestUndoable", () => {
  const h = (id: string, kind: string, undoable: boolean, undone = false, page: number | null = 0): HistoryEntry => ({ id, ts: "", kind, label: id, undone, undoable, page });
  it("takes the newest change not yet undone", () => {
    expect(latestUndoable([h("u", "undo", false), h("e2", "edit", true, true), h("l", "layout", false), h("e1", "edit", true)])?.id).toBe("e1");
    expect(latestUndoable([h("e", "edit", true, true)])).toBeNull();
    expect(latestUndoable([])).toBeNull();
  });
  it("passes over an approval removed afterwards", () => {
    expect(latestUndoable([h("un", "approve", false), h("ap", "approve", true), h("d", "decision", true)])?.id).toBe("d");
    expect(latestUndoable([h("un", "approve", false, false, 1), h("ap", "approve", true)])?.id).toBe("ap");
  });
});
