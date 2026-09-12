//! Standalone worker executable used by tests and tools (the app binary
//! serves the same role in production via `--worker`).
fn main() {
    let code = sbwb_worker::run_stdio_worker();
    std::process::exit(code);
}
