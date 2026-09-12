//! Settings commands (UX-06): per-book processing settings with re-run
//! counts, model pack status, storage facts.

use std::path::Path;

use sbwb_pipeline::ProcessingSettings;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, CmdResult, CommandError};

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<ProcessingSettings> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    match guard.as_ref() {
        Some(p) => Ok(ProcessingSettings::load(&p.store)?),
        None => Ok(ProcessingSettings::default()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunPreview {
    /// Pages whose OCR would be redone by these settings.
    pub rerun_pages: u32,
    /// Approved pages that are left untouched (PIPE-01).
    pub approved_untouched: u32,
    pub effective_workers: u32,
}

#[tauri::command]
pub fn settings_preview(
    state: State<'_, AppState>,
    settings: ProcessingSettings,
) -> CmdResult<RerunPreview> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    let fp = settings.ocr_fingerprint();
    let mut rerun = 0;
    let mut approved = 0;
    for page in p.store.pages()? {
        if page.status != sbwb_core::PageStatus::Done {
            continue;
        }
        if page.approved_revision.is_some() {
            approved += 1;
            continue;
        }
        let runs = p.store.runs_for_page(page.index)?;
        let same = runs
            .iter()
            .rev()
            .find(|r| r.stage == sbwb_core::Stage::Ocr && r.status == "ok")
            .and_then(|r| {
                r.settings
                    .get("fingerprint")
                    .and_then(|v| v.as_str())
                    .map(|s| s == fp)
            })
            .unwrap_or(false);
        if !same {
            rerun += 1;
        }
    }
    Ok(RerunPreview {
        rerun_pages: rerun,
        approved_untouched: approved,
        effective_workers: sbwb_pipeline::settings::effective_workers(settings.workers),
    })
}

/// Save settings; with `rerun`, re-queue affected pages and start processing.
#[tauri::command]
pub fn settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: ProcessingSettings,
    rerun: bool,
) -> CmdResult<u32> {
    let requeued = {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        settings.save(&p.store)?;
        p.store.add_history(
            "settings",
            "Processing settings changed",
            &serde_json::to_value(&settings)?,
        )?;
        if rerun {
            sbwb_pipeline::scheduler::requeue_for_settings(&p.store, &settings)?
        } else {
            0
        }
    };
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    if rerun && requeued > 0 {
        let _ = crate::commands::pipeline::start_pipeline(&app);
    }
    Ok(requeued)
}

#[tauri::command]
pub fn models_report(state: State<'_, AppState>) -> CmdResult<Vec<sbwb_ocr::models::PackReport>> {
    Ok(sbwb_ocr::models::report(&state.resources.tessdata_dir))
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageInfo {
    pub cache_dir: Option<String>,
    pub cache_bytes: u64,
    pub log_dir: String,
    pub tessdata_dir: String,
    pub tesseract_version: String,
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            if let Ok(md) = e.metadata() {
                if md.is_dir() {
                    total += dir_size(&e.path());
                } else {
                    total += md.len();
                }
            }
        }
    }
    total
}

#[tauri::command]
pub fn storage_info(app: AppHandle, state: State<'_, AppState>) -> CmdResult<StorageInfo> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let (cache_dir, cache_bytes) = match guard.as_ref() {
        Some(p) => {
            let d = p.store.cache_dir();
            (Some(d.to_string_lossy().to_string()), dir_size(&d))
        }
        None => (None, 0),
    };
    let log_dir = app
        .path()
        .app_log_dir()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(StorageInfo {
        cache_dir,
        cache_bytes,
        log_dir,
        tessdata_dir: state.resources.tessdata_dir.to_string_lossy().to_string(),
        tesseract_version: sbwb_ocr::Engine::version(),
    })
}

/// Delete rebuildable renders (never the source or the project).
#[tauri::command]
pub fn clear_render_cache(state: State<'_, AppState>) -> CmdResult<u64> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    let mut freed = 0;
    for sub in ["renders", "ocr-renders"] {
        let d = p.store.cache_dir().join(sub);
        if d.is_dir() {
            freed += dir_size(&d);
            let _ = std::fs::remove_dir_all(&d);
            let _ = std::fs::create_dir_all(&d);
        }
    }
    Ok(freed)
}
