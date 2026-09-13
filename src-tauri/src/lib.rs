mod commands;
mod logging;
mod paths;
mod render;
mod state;

use tauri::Manager;

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "identifier": "io.github.samueldotj.sbwb",
        "tesseract": sbwb_ocr::Engine::version(),
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
        .register_asynchronous_uri_scheme_protocol("sbwb-render", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(render::handle(&app, &request)));
        })
        .setup(|app| {
            let log_dir = app.path().app_log_dir()?;
            logging::init(&log_dir)?;
            let res = paths::Resources::locate(app.handle());
            tracing::info!(pdfium = %res.pdfium_dir.display(), tessdata = %res.tessdata_dir.display(), lexicons = %res.lexicon_dir.display(), "resources located");
            app.manage(state::AppState::new(res));
            app.manage(commands::pipeline::PipelineSlot::default());
            app.manage(commands::export::ExportSlot::default());
            app.manage(commands::ai::AiSlot::default());
            app.manage(commands::queue::QueueSlot::default());
            commands::queue::load(&app.handle().clone());

            // Writer-lock heartbeat (PRJ-02).
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("lock-heartbeat".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    commands::project::heartbeat(handle.state::<state::AppState>().inner());
                })?;

            #[cfg(debug_assertions)]
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.eval("window.__sbwbProbe && window.__sbwbProbe()");
                        if let Ok(p) = std::env::var("SBWB_DEV_IMPORT") {
                            let review = std::env::var("SBWB_DEV_REVIEW").ok().and_then(|v| v.parse::<u32>().ok());
                            let tab = std::env::var("SBWB_DEV_TAB").ok().map(|t| format!(".then(() => window.__sbwb.tab({}))", serde_json::to_string(&t).unwrap())).unwrap_or_default();
                            let eval_js = std::env::var("SBWB_DEV_EVAL").ok().map(|js| format!(".then(() => new Promise(r => setTimeout(r, 1500))).then(() => {{ {js} }})")).unwrap_or_default();
                            let then = review.map(|i| format!(".then(() => window.__sbwb.review({i})){tab}{eval_js}")).unwrap_or_default();
                            let js = format!("window.__sbwb && window.__sbwb.importPdf({}){then}", serde_json::to_string(&p).unwrap());
                            let _ = w.eval(&js);
                        }
                        // Eval alone (no import): drive an already-available book or the Welcome page.
                        if std::env::var_os("SBWB_DEV_IMPORT").is_none() {
                            if let Ok(js) = std::env::var("SBWB_DEV_EVAL") {
                                let _ = w.eval(&js);
                            }
                        }
                        if let Some(secs) = std::env::var("SBWB_DEV_LAYOUT_AT").ok().and_then(|v| v.parse::<u64>().ok()) {
                            let page = std::env::var("SBWB_DEV_REVIEW").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                            std::thread::sleep(std::time::Duration::from_secs(secs));
                            let _ = w.eval(format!("window.__sbwb && window.__sbwb.layout({page})"));
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Release the lock on close so another instance can write (PRJ-04
            // close flow with drafts arrives in M6).
            if let tauri::WindowEvent::Destroyed = event {
                commands::pipeline::stop_for_close(window.app_handle());
                if let Some(state) = window.try_state::<state::AppState>() {
                    if let Ok(mut guard) = state.project.lock() {
                        if let Some(p) = guard.take() {
                            let _ = p.store.close();
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            log_frontend,
            commands::project::inspect_source,
            commands::project::import_pdf,
            commands::project::open_project,
            commands::project::project_summary,
            commands::project::close_project,
            commands::project::save_copy,
            commands::project::set_scope,
            commands::page::page_ocr,
            commands::page::page_runs,
            commands::page::prefetch_render,
            commands::pipeline::pipeline_start,
            commands::pipeline::pipeline_pause,
            commands::pipeline::pipeline_resume,
            commands::pipeline::pipeline_cancel,
            commands::pipeline::pipeline_retry_failed,
            commands::pipeline::pipeline_status,
            commands::settings::settings_get,
            commands::settings::settings_preview,
            commands::settings::settings_set,
            commands::settings::models_report,
            commands::settings::storage_info,
            commands::settings::clear_render_cache,
            commands::layout::page_layout,
            commands::layout::layout_save,
            commands::layout::layout_rerun,
            commands::text::page_text,
            commands::text::text_pass_summary,
            commands::text::text_pass_rerun,
            commands::text::vocab_list,
            commands::text::vocab_add,
            commands::text::vocab_remove,
            commands::review::review_counts,
            commands::review::page_issues,
            commands::review::next_issue,
            commands::review::issue_decide,
            commands::review::span_flag,
            commands::review::flag_remove,
            commands::review::page_approve,
            commands::review::page_unapprove,
            commands::review::group_preview,
            commands::review::group_apply,
            commands::review::history_list,
            commands::review::history_undo,
            commands::review::draft_put,
            commands::review::draft_delete,
            commands::review::draft_list,
            commands::review::review_prefs_get,
            commands::review::review_prefs_set,
            commands::export::export_readiness,
            commands::export::export_preview,
            commands::export::export_run,
            commands::export::export_cancel,
            commands::export::export_defaults_get,
            commands::export::export_defaults_set,
            commands::export::open_path,
            commands::export::open_file,
            commands::export::exports_list,
            commands::refine::region_ocr,
            commands::refine::second_engine_available,
            commands::ai::ai_status,
            commands::ai::ai_key_set,
            commands::ai::ai_key_forget,
            commands::ai::ai_models,
            commands::ai::ai_estimate,
            commands::ai::ai_run,
            commands::ai::ai_cancel,
            commands::ai::ai_runs,
            commands::queue::queue_list,
            commands::queue::queue_add,
            commands::queue::queue_remove,
            commands::queue::queue_move,
            commands::queue::queue_retry,
            commands::queue::queue_clear_finished,
            commands::queue::queue_start,
            commands::queue::queue_stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SBWB");
}
