import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { invoke, isTauri } from "$lib/ipc";

function report(level: "error" | "warn" | "info", message: string) {
  if (isTauri) void invoke("log_frontend", { level, message }).catch(() => {});
  else if (level === "error") console.error(message);
}

window.addEventListener("error", (e) => report("error", `${e.message} @ ${e.filename}:${e.lineno}`));
function describe(v: unknown): string {
  if (v instanceof Error) return v.stack ?? v.message;
  if (typeof v === "object" && v !== null) {
    try {
      return JSON.stringify(v);
    } catch {
      return String(v);
    }
  }
  return String(v);
}
window.addEventListener("unhandledrejection", (e) => report("error", `unhandled rejection: ${describe(e.reason)}`));
(window as unknown as { __sbwbProbe: () => void }).__sbwbProbe = () =>
  report("info", `probe: body ${document.body.innerHTML.length} chars, app ${document.getElementById("app")?.childElementCount ?? -1} children`);

let app;
try {
  app = mount(App, { target: document.getElementById("app")! });
  report("info", "frontend mounted");
} catch (err) {
  report("error", `mount failed: ${err instanceof Error ? (err.stack ?? err.message) : String(err)}`);
  throw err;
}

export default app;
