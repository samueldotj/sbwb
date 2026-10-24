// App updates from GitHub releases. Checking is on request, or at startup
// when the user turns that on; nothing goes online otherwise (NFR-11).

import { api, errorMessage, type UpdateEvent, type UpdateInfo } from "$lib/api";
import { confirmDialog } from "$lib/dialogs";
import { isTauri, listen } from "$lib/ipc";
import { layout } from "./layout.svelte";
import { ui } from "./ui.svelte";

const AUTO_KEY = "sbwb.updateCheck";

function readAuto(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) === "on";
  } catch {
    return false;
  }
}

class UpdateState {
  /** Check for a new version each time the app starts. */
  checkAtStartup = $state<boolean>(readAuto());
  info = $state<UpdateInfo | null>(null);
  checking = $state(false);
  checkedAt = $state<number | null>(null);
  error = $state("");
  /** Download progress while the installer is fetched. */
  downloading = $state<{ done: number; total: number } | null>(null);
  #listening = false;

  /** A newer release this copy can install. */
  get available(): boolean {
    return !!this.info?.newer && !!this.info.installer;
  }

  setCheckAtStartup(on: boolean) {
    this.checkAtStartup = on;
    try {
      localStorage.setItem(AUTO_KEY, on ? "on" : "off");
    } catch {
      // Private storage unavailable; the choice lasts for this session.
    }
  }

  async check(quiet = false) {
    if (!isTauri || this.checking) return;
    this.checking = true;
    this.error = "";
    try {
      this.info = await api.updateCheck();
      this.checkedAt = Date.now();
      if (!quiet && !this.info.newer) ui.toast(`SBWB ${this.info.current} is the latest version`, "ok");
    } catch (e) {
      this.error = errorMessage(e);
      if (!quiet) ui.toast(`Update check failed: ${this.error}`, "error", 6000);
    } finally {
      this.checking = false;
    }
  }

  /** Download, verify and run the installer; the app quits when it starts. */
  async install() {
    const info = this.info;
    if (!info?.installer || this.downloading) return;
    if (layout.dirty) {
      ui.toast("Save or discard the layout changes before updating", "warn", 5000);
      return;
    }
    const size = (info.installer.size / 1024 / 1024).toFixed(1);
    const ok = await confirmDialog(
      `Download ${info.installer.name} (${size} MB) from GitHub and install SBWB ${info.latest}?\n\nSBWB closes while the installer runs and reopens when it finishes. Processing in progress stops at the current page; finished work is kept.`,
      "Update SBWB",
    );
    if (!ok) return;
    await this.#listen();
    this.error = "";
    this.downloading = { done: 0, total: info.installer.size };
    try {
      await api.updateInstall();
    } catch (e) {
      this.downloading = null;
      this.error = errorMessage(e);
      ui.toast(`Update failed: ${this.error}`, "error", 6000);
    }
  }

  async #listen() {
    if (this.#listening) return;
    this.#listening = true;
    await listen<UpdateEvent>("update:event", (e) => {
      if (e.type === "progress") {
        this.downloading = { done: e.done, total: e.total || this.info?.installer?.size || 0 };
      } else if (e.type === "downloaded") {
        this.downloading = null;
        ui.toast(e.launching ? "Installing the update…" : `Downloaded and verified (development build, not installed): ${e.path}`, "ok", e.launching ? 0 : 8000);
      } else if (e.type === "failed") {
        this.downloading = null;
        this.error = e.message;
        ui.toast(`Update failed: ${e.message}`, "error", 8000);
      }
    });
  }
}

export const update = new UpdateState();
