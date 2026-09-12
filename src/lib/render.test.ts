import { describe, expect, it } from "vitest";
import { fitZoom, nextZoom, pickScale, renderUrl } from "./render";

describe("render scale selection", () => {
  it("picks the smallest backend scale covering zoom times dpr", () => {
    expect(pickScale(1, 1)).toBe(1);
    expect(pickScale(1, 1.25)).toBe(1.5);
    expect(pickScale(0.3, 1)).toBe(0.35);
    expect(pickScale(10, 2)).toBe(6);
  });
  it("steps zoom through the ladder and clamps", () => {
    expect(nextZoom(1, 1)).toBe(1.25);
    expect(nextZoom(1, -1)).toBe(0.75);
    expect(nextZoom(6, 1)).toBe(6);
    expect(nextZoom(0.25, -1)).toBe(0.25);
    expect(nextZoom(1.1, 1)).toBe(1.25);
  });
  it("fits a page into a pane", () => {
    // 355x606 pt page in an 800x600 px pane fits by height
    expect(fitZoom(355, 606, 800, 600)).toBeCloseTo(600 / 606, 5);
    expect(fitZoom(355, 606, 800, 600, "width")).toBeCloseTo(800 / 355, 5);
  });
  it("builds protocol urls", () => {
    expect(renderUrl(12, 0.5)).toMatch(/sbwb-render(\.localhost|:\/\/localhost)\/p\/12\?s=0\.5$/);
  });
});
