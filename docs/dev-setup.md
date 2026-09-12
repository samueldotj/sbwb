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

Environment overrides: `SBWB_PDFIUM_DIR`, `SBWB_TESSDATA_DIR`, `SBWB_LOG`
(tracing filter, default `info`). Logs go to the Tauri app log directory
with the home path redacted.
