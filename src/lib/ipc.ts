// Thin wrapper over Tauri's invoke so components work in a plain browser
// (Vite dev server, Vitest) with the backend absent.

export const isTauri: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export class IpcUnavailable extends Error {
  constructor(cmd: string) {
    super(`IPC command "${cmd}" is unavailable outside the Tauri shell`);
  }
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) throw new IpcUnavailable(cmd);
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export async function listen<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<T>(event, (e) => handler(e.payload));
  return unlisten;
}
