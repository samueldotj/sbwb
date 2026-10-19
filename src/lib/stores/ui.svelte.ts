// App-wide UI state: theme, toasts. Svelte 5 runes in a .svelte.ts module.

export type Theme = "paper" | "bench";
export type TextSize = "normal" | "large";
export type SaveState = "idle" | "saving" | "saved" | "failed";

export type Toast = {
  id: number;
  text: string;
  kind: "info" | "ok" | "warn" | "error";
  timeoutMs: number;
};

const THEME_KEY = "sbwb.theme";
const SIZE_KEY = "sbwb.textsize";
const TRANSCRIPT_ZOOM_KEY = "sbwb.transcriptzoom";
/** Transcript zoom steps, in percent of the theme's transcript size. */
export const TRANSCRIPT_ZOOMS = [70, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 350, 400];

function readStoredTheme(): Theme {
  try {
    const v = localStorage.getItem(THEME_KEY);
    if (v === "paper" || v === "bench") return v;
  } catch {
    /* storage unavailable */
  }
  return "paper";
}

function readStoredSize(): TextSize {
  try {
    if (localStorage.getItem(SIZE_KEY) === "large") return "large";
  } catch {
    /* storage unavailable */
  }
  return "normal";
}

function readStoredTranscriptZoom(): number {
  try {
    const v = Number(localStorage.getItem(TRANSCRIPT_ZOOM_KEY));
    if (TRANSCRIPT_ZOOMS.includes(v)) return v;
  } catch {
    /* storage unavailable */
  }
  return 100;
}

class UiState {
  theme = $state<Theme>(readStoredTheme());
  /** Enlarged text for the whole interface (UX-05). */
  textSize = $state<TextSize>(readStoredSize());
  /** Transcript text zoom in percent, independent of the scan zoom. */
  transcriptZoom = $state<number>(readStoredTranscriptZoom());
  /** Polite live-region text for screen readers (UX-05). */
  announcement = $state("");
  /** Durable-write state shown in the status bar (PRJ-04). */
  saveState = $state<SaveState>("idle");
  savedAt = $state<number | null>(null);
  saveError = $state("");
  #pending = 0;
  toasts = $state<Toast[]>([]);
  settingsOpen = $state(false);
  /// Inspector tab requested from outside the workspace (rail rows, dev hooks).
  inspectorTab = $state<string | null>(null);
  /// The Export sheet (design 4.5).
  exportOpen = $state(false);
  /// The book queue sheet (PRJ-06).
  queueOpen = $state(false);
  #nextToast = 1;

  constructor() {
    $effect.root(() => {
      $effect(() => {
        document.documentElement.dataset.theme = this.theme;
        try {
          localStorage.setItem(THEME_KEY, this.theme);
        } catch {
          /* ignore */
        }
      });
      $effect(() => {
        document.documentElement.dataset.textsize = this.textSize;
        try {
          localStorage.setItem(SIZE_KEY, this.textSize);
        } catch {
          /* ignore */
        }
      });
      $effect(() => {
        try {
          localStorage.setItem(TRANSCRIPT_ZOOM_KEY, String(this.transcriptZoom));
        } catch {
          /* ignore */
        }
      });
    });
  }

  announce(text: string) {
    // Re-set so identical messages are announced again.
    this.announcement = "";
    setTimeout(() => (this.announcement = text), 30);
  }

  /** Wrap a durable write so the status bar reflects it truthfully. */
  async save<T>(work: () => Promise<T>): Promise<T> {
    this.#pending++;
    this.saveState = "saving";
    try {
      const r = await work();
      this.#pending--;
      if (this.#pending === 0) {
        this.saveState = "saved";
        this.savedAt = Date.now();
        this.saveError = "";
      }
      return r;
    } catch (e) {
      this.#pending--;
      this.saveState = "failed";
      this.saveError = e instanceof Error ? e.message : typeof e === "object" && e && "message" in e ? String((e as { message: unknown }).message) : String(e);
      throw e;
    }
  }

  toggleTheme() {
    this.theme = this.theme === "paper" ? "bench" : "paper";
  }

  /** Step the transcript zoom; 0 resets it to 100%. */
  zoomTranscript(dir: -1 | 0 | 1) {
    if (dir === 0) {
      this.transcriptZoom = 100;
      return;
    }
    const i = TRANSCRIPT_ZOOMS.indexOf(this.transcriptZoom);
    const next = TRANSCRIPT_ZOOMS[Math.max(0, Math.min(TRANSCRIPT_ZOOMS.length - 1, (i < 0 ? 3 : i) + dir))];
    if (next !== undefined) this.transcriptZoom = next;
  }

  toast(text: string, kind: Toast["kind"] = "info", timeoutMs = 2500) {
    const id = this.#nextToast++;
    this.toasts = [...this.toasts, { id, text, kind, timeoutMs }];
    if (timeoutMs > 0) setTimeout(() => this.dismiss(id), timeoutMs);
    return id;
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const ui = new UiState();
