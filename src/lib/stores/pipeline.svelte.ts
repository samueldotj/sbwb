// Live pipeline state from `pipeline:event` (PIPE-02, design 4.2).

import { api, errorMessage } from "$lib/api";
import { isTauri, listen } from "$lib/ipc";
import { ui } from "./ui.svelte";

export type PipelineState = "idle" | "running" | "paused" | "stopping" | "done";

export type StageProgress = {
  stage: string;
  total: number;
  done: number;
  failed: number;
  running: number;
  elapsed_ms: number;
  eta_ms: number | null;
  secs_per_unit: number | null;
};

export type LogLine = { ts: string; level: string; text: string };

type Event =
  | { type: "state"; state: PipelineState }
  | { type: "progress"; stages: StageProgress[]; activity: string }
  | { type: "unit"; stage: string; page: number; ok: boolean; elapsed_ms: number; words: number | null; mean_confidence: number | null; error: string | null; warnings: string[] }
  | { type: "log"; ts: string; level: string; text: string }
  | { type: "finished"; done: number; failed: number; cancelled: boolean };

const LOG_MAX = 300;

class PipelineStore {
  state = $state<PipelineState>("idle");
  stages = $state<StageProgress[]>([]);
  activity = $state("");
  workers = $state(0);
  log = $state<LogLine[]>([]);
  #unlisten: (() => void) | null = null;

  get ocr(): StageProgress | null {
    return this.stages.find((s) => s.stage === "ocr") ?? null;
  }
  get layout(): StageProgress | null {
    return this.stages.find((s) => s.stage === "layout") ?? null;
  }
  get text(): StageProgress | null {
    return this.stages.find((s) => s.stage === "text_pass") ?? null;
  }
  /** The stage currently doing work, for the status bar. */
  get current(): StageProgress | null {
    const running = this.stages.find((s) => s.running > 0);
    if (running) return running;
    return this.stages.find((s) => s.done + s.failed < s.total) ?? this.ocr;
  }
  get active(): boolean {
    return this.state === "running" || this.state === "paused" || this.state === "stopping";
  }

  async init() {
    if (!isTauri) return;
    this.#unlisten = await listen<Event>("pipeline:event", (e) => this.apply(e));
    await this.refresh();
  }

  async refresh() {
    try {
      const s = await api.pipelineStatus();
      if (s.status) {
        this.state = s.active ? s.status.state : s.status.state === "done" ? "done" : "idle";
        this.stages = s.status.stages;
        this.activity = s.status.activity;
        this.workers = s.status.workers;
      }
    } catch {
      /* no book */
    }
  }

  apply(e: Event) {
    switch (e.type) {
      case "state":
        this.state = e.state;
        break;
      case "progress":
        this.stages = e.stages;
        this.activity = e.activity;
        break;
      case "log":
        this.log = [{ ts: e.ts, level: e.level, text: e.text }, ...this.log].slice(0, LOG_MAX);
        break;
      case "finished":
        this.state = e.cancelled ? "idle" : "done";
        if (e.failed > 0) ui.toast(`Processing finished: ${e.done} pages, ${e.failed} failed`, "warn", 6000);
        else if (!e.cancelled) ui.toast(`Processing finished: ${e.done} pages`, "ok");
        break;
      case "unit":
        break;
    }
  }

  reset() {
    this.state = "idle";
    this.stages = [];
    this.activity = "";
    this.log = [];
  }

  async start() {
    try {
      await api.pipelineStart();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  async pause() {
    await api.pipelinePause().catch((e) => ui.toast(errorMessage(e), "error"));
  }
  async resume() {
    await api.pipelineResume().catch((e) => ui.toast(errorMessage(e), "error"));
  }
  async cancel() {
    await api.pipelineCancel().catch((e) => ui.toast(errorMessage(e), "error"));
  }
  async retryFailed() {
    await api.pipelineRetryFailed().catch((e) => ui.toast(errorMessage(e), "error", 6000));
  }

  destroy() {
    this.#unlisten?.();
  }
}

export const pipeline = new PipelineStore();

export function formatEta(ms: number | null): string {
  if (ms === null) return "";
  const s = Math.round(ms / 1000);
  if (s < 60) return `~${s}s left`;
  const m = Math.round(s / 60);
  return m < 60 ? `~${m} min left` : `~${Math.round(m / 60)} h left`;
}
