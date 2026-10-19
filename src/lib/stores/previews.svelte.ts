// Background preview generation for the Pages view (IMG-01): progress from
// `previews:progress`, and start / stop. A stop by the user holds until the
// thumbnail size changes or Generate is pressed.

import { api, errorMessage } from "$lib/api";
import { isTauri, listen } from "$lib/ipc";
import { ui } from "./ui.svelte";

type Progress = { run: number; done: number; total: number; running: boolean };

class PreviewsStore {
  done = $state(0);
  total = $state(0);
  running = $state(false);
  /** The user pressed Stop; automatic starts wait for Generate or a new size. */
  stopped = $state(false);
  #run = 0;
  #unlisten: (() => void) | null = null;

  async init() {
    if (!isTauri || this.#unlisten) return;
    this.#unlisten = await listen<Progress>("previews:progress", (p) => {
      // a replaced run still reports its end; only the current one counts
      if (p.run === this.#run) this.#apply(p);
    });
  }

  #apply(p: Progress) {
    this.done = p.done;
    this.total = p.total;
    this.running = p.running;
  }

  /** Render these pages at these scales into the preview cache. */
  async start(items: { page: number; scale: number }[], byUser = false) {
    if (!isTauri) return;
    if (byUser) this.stopped = false;
    if (this.stopped) return;
    try {
      const p = await api.previewsStart(items);
      this.#run = p.run;
      this.#apply(p);
    } catch (e) {
      ui.toast(`Could not generate previews: ${errorMessage(e)}`, "error", 6000);
    }
  }

  async stop() {
    this.stopped = true;
    this.running = false;
    if (isTauri) await api.previewsStop().catch(() => {});
  }

  reset() {
    this.done = 0;
    this.total = 0;
    this.running = false;
    this.stopped = false;
  }
}

export const previews = new PreviewsStore();
