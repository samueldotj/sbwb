mod logging;
mod paths;

use tauri::Manager;

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "identifier": "io.github.samueldotj.sbwb",
    })
}

/// Frontend errors and diagnostics land in the same redacted log (NFR-11).
#[tauri::command]
fn log_frontend(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{message}"),
        "warn" => tracing::warn!(target: "frontend", "{message}"),
        _ => tracing::info!(target: "frontend", "{message}"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let log_dir = app.path().app_log_dir()?;
            logging::init(&log_dir)?;
            let res = paths::Resources::locate(app.handle());
            tracing::info!(pdfium = %res.pdfium_dir.display(), tessdata = %res.tessdata_dir.display(), "resources located");
            app.manage(res);
            #[cfg(debug_assertions)]
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.eval("window.__sbwbProbe && window.__sbwbProbe()");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, log_frontend])
        .run(tauri::generate_context!())
        .expect("error while running SBWB");
}
