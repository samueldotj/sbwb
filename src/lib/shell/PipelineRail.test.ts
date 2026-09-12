import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import PipelineRail from "./PipelineRail.svelte";

const stages = [
  { id: "import", label: "Import", state: "done" as const, value: "50 pp" },
  { id: "ocr", label: "OCR", state: "running" as const, value: "14 / 50", progress: 0.28 },
  { id: "layout", label: "Layout", state: "queued" as const, value: "queued" },
];

describe("PipelineRail", () => {
  it("renders every stage as an accessible button and exposes progress", () => {
    render(PipelineRail, { props: { stages } });
    expect(screen.getByRole("navigation", { name: "Pipeline" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Import/ })).toBeInTheDocument();
    const bar = screen.getByRole("progressbar", { name: "OCR progress" });
    expect(bar).toHaveAttribute("aria-valuenow", "28");
  });

  it("reports selection", async () => {
    const onselect = vi.fn();
    render(PipelineRail, { props: { stages, onselect } });
    await fireEvent.click(screen.getByRole("button", { name: /Layout/ }));
    expect(onselect).toHaveBeenCalledWith("layout");
  });
});
