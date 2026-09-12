# Developer setup (Windows)

1. Rust stable (MSVC), Node 22+, Visual Studio 2022 Build Tools with the
   "Desktop development with C++" workload.
2. vcpkg for Tesseract and Leptonica (one-time, 30-60 minutes):

   ```powershell
   git clone https://github.com/microsoft/vcpkg $HOME\vcpkg
   & $HOME\vcpkg\bootstrap-vcpkg.bat -disableMetrics
   & $HOME\vcpkg\vcpkg.exe install tesseract:x64-windows-static-md leptonica:x64-windows-static-md
   [Environment]::SetEnvironmentVariable("VCPKG_ROOT", "$HOME\vcpkg", "User")
   ```

3. libclang for the Tesseract/Leptonica bindings (bindgen). The smallest
   route is the `libclang` Python wheel; LLVM from winget also works:

   ```powershell
   pip install libclang
   [Environment]::SetEnvironmentVariable("LIBCLANG_PATH", (python -c "import clang,os;print(os.path.join(os.path.dirname(clang.__file__),'native'))"), "User")
   ```

4. Fetch PDFium and the Tesseract models (checksummed, git-ignored):

   ```powershell
   ./scripts/fetch-deps.ps1
   ```

5. Install frontend dependencies and run:

   ```powershell
   npm ci
   npm run tauri dev
   ```

Tests: `cargo test --workspace` and `npm test`. The Rust tests that need
PDFium or models skip themselves with a message when the files are absent.

Environment overrides: `SBWB_PDFIUM_DIR`, `SBWB_TESSDATA_DIR`, `SBWB_LEXICON_DIR` (a flat folder with `en_GB-large.aff`, `en_GB-large.dic`, `webster-1913-headwords.txt`; in dev `scripts/fetch-deps.ps1` makes `fixtures/lexicons/dev/`), `SBWB_LOG`
(tracing filter, default `info`). Logs go to the Tauri app log directory
with the home path redacted.

## Dev hooks

Debug builds expose `window.__sbwb` (`importPdf`, `open`, `close`, `review`,
`layout`, `tab`) and read these variables at start-up, about 8 s after the
window opens:

| Variable | Effect |
| --- | --- |
| `SBWB_DEV_IMPORT=<pdf>` | Import the file (first 50 pages) and start processing |
| `SBWB_DEV_REVIEW=<index>` | Then open that page (0-based) in Review |
| `SBWB_DEV_TAB=<id>` | Then select an inspector tab (`import`, `page`, `text_pass`, `ai`) |
| `SBWB_DEV_EVAL=<js>` | Then run that JavaScript in the page (after 1.5 s); `window.__sbwb.step/decide/approve` drive the review loop |
| `SBWB_DEV_LAYOUT_AT=<secs>` | Enter Layout mode for the review page after that many seconds |

`SBWB_PERF=1 cargo test -p sbwb-export large_fixture` enforces the 60 s export budget (NFR-05) instead of only reporting the time.

`scripts/capture-window.ps1 -Out shot.png` grabs the window without taking
focus. The Vite dev server alone (`npm run dev`, then `/?mock=review`) renders
the workspace with mock data and no backend.

