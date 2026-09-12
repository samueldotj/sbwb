// App-wide UI state: theme, toasts. Svelte 5 runes in a .svelte.ts module.

export type Theme = "paper" | "bench";

export type Toast = {
  id: number;
  text: string;
  kind: "info" | "ok" | "warn" | "error";
  timeoutMs: number;
};

const THEME_KEY = "sbwb.theme";

function readStoredTheme(): Theme {
  try {
    const v = localStorage.getItem(THEME_KEY);
    if (v === "paper" || v === "bench") return v;
  } catch {
    /* storage unavailable */
  }
  return "paper";
}

class UiState {
  theme = $state<Theme>(readStoredTheme());
  toasts = $state<Toast[]>([]);
  settingsOpen = $state(false);
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
    });
  }

  toggleTheme() {
    this.theme = this.theme === "paper" ? "bench" : "paper";
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
