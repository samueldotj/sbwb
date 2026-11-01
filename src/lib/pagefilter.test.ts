import { describe, expect, it } from "vitest";
import type { PageRow } from "$lib/api";
import { filterPages, isUnreviewed, nearestUnreviewed } from "./pagefilter";

const page = (index: number, status: PageRow["status"], approval: PageRow["approval"]) => ({ index, status, approval }) as PageRow;

const pages = [
  page(0, "done", "current"),
  page(1, "done", "none"),
  page(2, "done", "outdated"),
  page(3, "unprocessed", "none"),
  page(4, "excluded", "none"),
  page(5, "failed", "none"),
  page(6, "done", "current"),
];
const indexes = (rows: PageRow[]) => rows.map((p) => p.index);

describe("page filter", () => {
  it("treats outdated approvals as unreviewed and out-of-scope pages as not", () => {
    expect(indexes(pages.filter(isUnreviewed))).toEqual([1, 2, 5]);
  });

  it("lists every page when off", () => {
    expect(filterPages(pages, false)).toBe(pages);
  });

  it("keeps the given page listed when on", () => {
    expect(indexes(filterPages(pages, true))).toEqual([1, 2, 5]);
    expect(indexes(filterPages(pages, true, 0))).toEqual([0, 1, 2, 5]);
  });

  it("finds the nearest unreviewed page in either direction", () => {
    expect(nearestUnreviewed(pages, 2, 1)).toBe(5);
    expect(nearestUnreviewed(pages, 5, -1)).toBe(2);
    expect(nearestUnreviewed(pages, 5, 1)).toBeNull();
    expect(nearestUnreviewed(pages, 1, -1)).toBeNull();
  });
});
