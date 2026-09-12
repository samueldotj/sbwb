// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The same executable serves as the isolated worker process (M0.9,
    // NFR-11). Nothing from the UI side is initialised in that mode.
    if std::env::args().any(|a| a == "--worker") {
        let code = sbwb_worker::run_stdio_worker();
        std::process::exit(code);
    }
    sbwb_app_lib::run()
}
