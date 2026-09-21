# Page navigation 2026-09-20

Measured values on this machine; they are not advertised results (NFR-02).
Produced by `scripts/nav-bench.sh` (M2.6): the time from asking for a page
to the scan image of that page being complete in the DOM, in the running app.

| Item | Value |
| --- | --- |
| Host | sam-desktop2025 |
| OS | Windows 11 Home x86_64 |
| CPU | AMD Ryzen 7 9800X3D 8-Core Processor (16 logical cores) |
| Memory | 61.6 GB |
| Build | debug app against the Vite dev server · parent commit c6afe67 plus the prefetch changes below |
| Book | `fixtures/corpus/hough-1839-vol1/pages-001-110.pdf`, processed project, 110 pages |
| View | window 3128 × 1266 CSS px, device pixel ratio 1.10, fit zoom, backend render scale 2.0 |
| Cache state | render cache deleted before the run |

## Result (NFR-03)

| Measure | n | p50 | p95 | max | Target (p95) |
| --- | --- | --- | --- | --- | --- |
| Uncached jump (three pages ahead, never prefetched) | 30 | 278 ms | 325 ms | 333 ms | ≤ 2000 ms |
| Cached revisit of the same pages | 30 | 12 ms | 48 ms | 57 ms | ≤ 250 ms |
| Next page, neighbour prefetched | 30 | 25 ms | 104 ms | 160 ms | ≤ 250 ms |

All three are inside the targets. The earlier `sbwb-bench` interaction
figures (129 ms uncached render at scale 1.5, 3.8 ms cached read) time the
backend alone; these include the webview.

## What the first run found

The first run of this script, before any change, gave:

| Measure | p50 | p95 | max |
| --- | --- | --- | --- |
| Uncached jump | 666 ms | 1672 ms | 1688 ms |
| Cached revisit | 12 ms | 91 ms | 93 ms |
| Next page | 334 ms | 583 ms | 638 ms |

Next-page navigation was not hitting the prefetch at all, and jumps were
slow. Three causes, all fixed in the same change:

1. The neighbour prefetch assumed zoom 1 in fit and width modes, so on a
   large window it warmed scale 1.5 while the pane asked for scale 2. The
   pane now publishes the scale it uses (`view.renderScale`) and the
   prefetch follows it.
2. `prefetch_render` was a synchronous command, which Tauri runs on the
   main thread, so each prefetch stalled the window for the length of a
   render. It now returns at once and renders on its own thread.
3. A render for the page on screen queued behind prefetch renders on the
   one render worker, and even a cache hit took the worker lock. Cache hits
   are now served without the lock (renders are written with a temporary
   file and a rename), and a prefetch gives way when a render for the page
   on screen is waiting, so navigation waits for at most the one prefetch
   already running.
