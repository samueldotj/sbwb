// Native file dialogs (tauri-plugin-dialog). Return null when cancelled or
// when running outside the shell.

import { isTauri } from "./ipc";

export async function pickPdf(): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false, title: "Choose a PDF", filters: [{ name: "PDF", extensions: ["pdf"] }] });
  return typeof r === "string" ? r : null;
}

export async function pickProject(): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, directory: false, title: "Open a .sbwb project", filters: [{ name: "SBWB project", extensions: ["sbwb"] }] });
  return typeof r === "string" ? r : null;
}

export async function pickSaveCopy(defaultPath?: string): Promise<string | null> {
  if (!isTauri) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const r = await save({ title: "Save a copy of the project", defaultPath, filters: [{ name: "SBWB project", extensions: ["sbwb"] }] });
  return r ?? null;
}

export async function confirmDialog(message: string, title = "SBWB"): Promise<boolean> {
  if (!isTauri) return window.confirm(message);
  const { confirm } = await import("@tauri-apps/plugin-dialog");
  return confirm(message, { title, kind: "warning" });
}
