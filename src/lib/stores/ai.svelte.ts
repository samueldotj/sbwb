// The AI proofreading run (AI-02), kept for the whole app so its progress
// and finish notice carry on when the AI tab is closed. The run itself is
// on a background thread in the app; this follows its events.

import { api, type AiProgress, type AiRunRecord } from "$lib/api";
import { isTauri, listen } from "$lib/ipc";
import { review } from "./review.svelte";
import { ui } from "./ui.svelte";

type AiEvent =
  | { type: "progress"; page: number; done: number; total: number }
  | { type: "page_done"; page: number; suggestions: number; rejected: number }
  | { type: "finished"; run: AiRunRecord }
  | { type: "failed"; run: AiRunRecord; error: { message: string } };

class AiState {
  /** The run in progress, or null. */
  running = $state<AiProgress | null>(null);
  /** Why the last run or send stopped; cleared by the next send. */
  lastError = $state("");
  /** Counts runs that ended, so views can reload status and history. */
  ended = $state(0);
  #listening = false;

  async init() {
    if (!isTauri || this.#listening) return;
    this.#listening = true;
    await listen<AiEvent>("ai:event", (e) => this.#onEvent(e));
    // a run started before this window loaded (a reload during a run)
    try {
      const status = await api.aiStatus();
      if (status.running && !this.running) this.running = status.running;
    } catch {
      // no status; the next progress event sets it
    }
  }

  /** Send was accepted; progress events take over from here. */
  started(firstPage: number, total: number) {
    this.running ??= { page: firstPage, done: 0, total };
  }

  #onEvent(e: AiEvent) {
    if (e.type === "progress") this.running = { page: e.page, done: e.done, total: e.total };
    else if (e.type === "page_done") {
      if (this.running) this.running = { ...this.running, done: Math.min(this.running.done + 1, this.running.total) };
      ui.announce(`Page ${e.page + 1}: ${e.suggestions} AI suggestions`);
    } else if (e.type === "finished") {
      this.running = null;
      this.ended++;
      ui.toast(`AI check finished: ${e.run.suggestions} suggestions on ${e.run.pages.length} page(s)`, "ok", 6000);
      void review.reload();
    } else if (e.type === "failed") {
      this.running = null;
      this.ended++;
      this.lastError = e.error.message;
      ui.toast(`AI check stopped: ${this.lastError}`, "error", 8000);
      void review.reload();
    }
  }
}

export const ai = new AiState();
