#!/bin/bash
# Page navigation timing in the running app (M2.6, NFR-03): the time from
# asking for a page to the scan image of that page being in the DOM.
#
# Usage (Git Bash, repository root, Vite dev server running, debug app built):
#   scripts/nav-bench.sh path/to/book.sbwb
#
# The book needs at least 100 pages. The render cache beside it is cleared
# first so the uncached pass is cold. Results are printed and also land in
# the app log as NAVBENCH lines.
set -u
book="${1:?usage: scripts/nav-bench.sh path/to/book.sbwb}"
book="$(cd "$(dirname "$book")" && pwd -W 2>/dev/null || pwd)/$(basename "$book")"
log="$(mktemp)"
taskkill //F //IM sbwb-app.exe >/dev/null 2>&1
sleep 1
rm -rf "$book.cache/renders"

export SBWB_DEV_EVAL='
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const say = (m) => invoke("log_frontend", { level: "info", message: "NAVBENCH " + m });
  await window.__sbwb.open("__BOOK__");
  await wait(3000);
  window.__sbwb.review(0);
  await wait(2500);
  const img = () => document.querySelector("img[alt^=\"Scan of page\"]");
  const shown = (i) => {
    const el = img();
    return !!el && el.alt === "Scan of page " + (i + 1) && el.src.includes("/p/" + i + "?") && el.complete;
  };
  // Poll with a message channel: timers and animation frames are throttled
  // when the window is not in front.
  const turn = () => new Promise((r) => { const c = new MessageChannel(); c.port1.onmessage = r; c.port2.postMessage(0); });
  const nav = async (i) => {
    const t = performance.now();
    window.__sbwb.review(i);
    while (!shown(i)) {
      await turn();
      if (performance.now() - t > 15000) throw new Error("page " + i + " did not appear");
    }
    return performance.now() - t;
  };
  const stats = (xs) => {
    const s = [...xs].sort((a, b) => a - b);
    const q = (p) => s[Math.min(s.length - 1, Math.ceil(p * s.length) - 1)];
    return "n=" + s.length + " p50=" + q(0.5).toFixed(1) + "ms p95=" + q(0.95).toFixed(1) + "ms max=" + s[s.length - 1].toFixed(1) + "ms";
  };
  // Jumps of three pages are never covered by the neighbour prefetch.
  const targets = [];
  for (let i = 10; i < 100; i += 3) targets.push(i);
  const cold = [], warm = [], seq = [];
  for (const i of targets) { cold.push(await nav(i)); await wait(500); }
  for (const i of targets) { warm.push(await nav(i)); await wait(150); }
  await nav(19); await wait(800);
  for (let i = 20; i < 50; i++) { seq.push(await nav(i)); await wait(400); }
  await say("view " + new URL(img().src).search + " window " + innerWidth + "x" + innerHeight + " dpr " + devicePixelRatio.toFixed(2));
  await say("uncached-jump " + stats(cold));
  await say("cached-revisit " + stats(warm));
  await say("next-page-prefetched " + stats(seq));
  await say("done");
})().catch((e) => window.__TAURI_INTERNALS__.invoke("log_frontend", { level: "error", message: "NAVBENCH failed " + e }));
'
SBWB_DEV_EVAL="${SBWB_DEV_EVAL//__BOOK__/$book}"

./target/debug/sbwb-app.exe >"$log" 2>&1 &
for _ in $(seq 1 60); do
  sleep 3
  grep -q "NAVBENCH \(done\|failed\)" "$log" && break
done
grep -o "NAVBENCH .*" "$log"
taskkill //F //IM sbwb-app.exe >/dev/null 2>&1
rm -f "$log"
