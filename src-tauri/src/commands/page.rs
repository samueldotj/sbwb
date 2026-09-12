//! Per-page queries: OCR evidence and run history (PROV-01, REV-06).

use sbwb_core::PageIndex;
use sbwb_store::RunRecord;
use serde::Serialize;
use tauri::State;

use crate::state::{AppState, CmdResult, CommandError};

#[derive(Debug, Clone, Serialize)]
pub struct PageOcrView {
    pub run_id: String,
    pub words: Vec<sbwb_ocr::OcrWord>,
    pub lines: Vec<sbwb_ocr::OcrLine>,
    pub mean_confidence: f32,
}

#[tauri::command]
pub fn page_ocr(state: State<'_, AppState>, index: u32) -> CmdResult<Option<PageOcrView>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    Ok(p.store
        .current_page_ocr(PageIndex(index))?
        .map(|(run, words, lines, conf)| PageOcrView {
            run_id: run.to_string(),
            words,
            lines,
            mean_confidence: conf,
        }))
}

#[tauri::command]
pub fn page_runs(state: State<'_, AppState>, index: u32) -> CmdResult<Vec<RunRecord>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    Ok(p.store.runs_for_page(PageIndex(index))?)
}

/// Pre-render a page at a scale so navigation to it is instant (used for
/// neighbours of the current page).
#[tauri::command]
pub fn prefetch_render(state: State<'_, AppState>, index: u32, scale: f64) -> CmdResult<()> {
    let (source, cache) = {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        (p.source_path.clone(), p.render_cache.clone())
    };
    state.with_render_worker(|w| cache.ensure(w, &source, PageIndex(index), scale))?;
    Ok(())
}
