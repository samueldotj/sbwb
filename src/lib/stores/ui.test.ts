import { describe, expect, it } from "vitest";
import { flushSync } from "svelte";
import { ui } from "./ui.svelte";

describe("ui store", () => {
  it("toggles theme and stamps the document", () => {
    const start = ui.theme;
    ui.toggleTheme();
    flushSync();
    expect(ui.theme).not.toBe(start);
    expect(document.documentElement.dataset.theme).toBe(ui.theme);
    ui.toggleTheme();
    flushSync();
    expect(ui.theme).toBe(start);
  });

  it("adds and dismisses toasts", () => {
    const id = ui.toast("Saved", "ok", 0);
    expect(ui.toasts.some((t) => t.id === id && t.text === "Saved")).toBe(true);
    ui.dismiss(id);
    expect(ui.toasts.some((t) => t.id === id)).toBe(false);
  });
});
