// App updates from GitHub releases. Checking is on request, or at startup
// when the user turns that on; nothing goes online otherwise (NFR-11).

import { api, errorMessage, type UpdateEvent, type UpdateInfo } from "$lib/api";
import { confirmDialog } from "$lib/dialogs";
import { isTauri, listen } from "$lib/ipc";
import { layout } from "./layout.svelte";
import { pipeline } from "./pipeline.svelte";
import { project } from "./project.svelte";
import { ui } from "./ui.svelte";

/** What happens at startup: nothing, a notice, or download and install. */
export type UpdateMode = "off" | "notify" | "install";

const MODE_KEY = "sbwb.updateMode";
const OLD_KEY = "sbwb.updateCheck";

function readMode(): UpdateMode {
  try {
    const v = localStorage.getItem(MODE_KEY);
    if (v === "off" || v === "notify" || v === "install") return v;
    return localStorage.getItem(OLD_KEY) === "on" ? "notify" : "off";
  } catch {
    return "off";
  }
}

class UpdateState {
  startupMode = $state<UpdateMode>(readMode());
  info = $state<UpdateInfo | null>(null);
  checking = $state(false);
  checkedAt = $state<number | null>(null);
  error = $state("");
  /** Download progress while the installer is fetched. */
  downloading = $state<{ done: number; total: number } | null>(null);
  /** Version whose verified installer is waiting to run. */
  ready = $state<string | null>(null);
  #listening = false;
  /** Install as soon as the download is verified, if nothing is open. */
  #autoApply = false;
  /** The lasting "Installing…" toast, cleared if the install fails. */
  #installingToast: number | null = null;

  /** A newer release this copy can install. */
  get available(): boolean {
    return !!this.info?.newer && !!this.info.installer;
  }

  setStartupMode(mode: UpdateMode) {
    this.startupMode = mode;
    try {
      localStorage.setItem(MODE_KEY, mode);
      localStorage.removeItem(OLD_KEY);
    } catch {
      // Private storage unavailable; the choice lasts for this session.
    }
  }

  /** Startup: check, and in install mode fetch and run a newer version. */
  async startup() {
    if (!isTauri || this.startupMode === "off") return;
    await this.check(true);
    if (this.startupMode !== "install" || !this.available || this.downloading) return;
    this.#autoApply = true;
    await this.#download(false);
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
    if (this.ready) {
      const ok = await confirmDialog(`Install SBWB ${this.ready} now?\n\nSBWB closes while the installer runs and reopens when it finishes. Processing in progress stops at the current page; finished work is kept.`, "Update SBWB");
      if (ok) await this.apply();
      return;
    }
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
    await this.#download(true);
  }

  /** Run the downloaded installer and quit. */
  async apply() {
    if (layout.dirty) {
      ui.toast("Save or discard the layout changes before updating", "warn", 5000);
      return;
    }
    try {
      await api.updateApply();
    } catch (e) {
      this.#clearInstalling();
      this.error = errorMessage(e);
      ui.toast(`Update failed: ${this.error}`, "error", 8000);
    }
  }

  #clearInstalling() {
    if (this.#installingToast !== null) ui.dismiss(this.#installingToast);
    this.#installingToast = null;
  }

  /** Nothing would be interrupted by quitting now. */
  #idle(): boolean {
    return !project.isOpen && !layout.dirty && pipeline.state !== "running";
  }

  async #download(launch: boolean) {
    await this.#listen();
    this.error = "";
    this.downloading = { done: 0, total: this.info?.installer?.size ?? 0 };
    try {
      await api.updateInstall(launch);
    } catch (e) {
      this.downloading = null;
      this.#autoApply = false;
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
      } else if (e.type === "ready") {
        this.downloading = null;
        this.ready = e.version;
        if (!this.#autoApply) return;
        this.#autoApply = false;
        if (this.#idle()) {
          this.#installingToast = ui.toast(`Installing SBWB ${e.version}…`, "ok", 0);
          void this.apply();
        } else {
          ui.toast(`SBWB ${e.version} is downloaded; install it from the status bar when you are ready`, "ok", 8000);
        }
      } else if (e.type === "installing") {
        this.#clearInstalling();
        this.#installingToast = ui.toast("Installing the update…", "ok", 0);
      } else if (e.type === "failed") {
        this.#clearInstalling();
        this.downloading = null;
        this.#autoApply = false;
        this.error = e.message;
        ui.toast(`Update failed: ${e.message}`, "error", 8000);
      }
    });
  }
}

export const update = new UpdateState();
