// Book-wide text-pass counters (design 3 status bar, 4.6 Text pass tab).
// Refreshed on project:changed, which the pipeline coalesces to ~300 ms.

import { api, type TextPassSummary } from "$lib/api";
import { isTauri, listen } from "$lib/ipc";

class TextPassStore {
  summary = $state<TextPassSummary | null>(null);
  #unlisten: (() => void) | null = null;

  async init() {
    if (!isTauri) return;
    this.#unlisten = await listen("project:changed", () => void this.refresh());
    await this.refresh();
  }

  async refresh() {
    try {
      this.summary = await api.textPassSummary();
    } catch {
      this.summary = null;
    }
  }

  reset() {
    this.summary = null;
  }

  destroy() {
    this.#unlisten?.();
  }
}

export const textPass = new TextPassStore();
