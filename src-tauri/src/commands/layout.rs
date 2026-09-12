//! Layout commands (LAY-02): read, save a manual layout, re-run analysis.

use sbwb_core::{PageIndex, PageStatus, Stage};
use sbwb_layout::Region;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, CmdResult, CommandError};

#[derive(Debug, Clone, Serialize)]
pub struct PageLayoutView {
    pub page: u32,
    pub regions: Vec<Region>,
    pub report: serde_json::Value,
    pub algorithm: Option<String>,
    pub manual: bool,
    pub revision: u64,
}

fn view(l: sbwb_store::PageLayout) -> PageLayoutView {
    let regions: Vec<Region> = serde_json::from_value(l.regions).unwrap_or_default();
    PageLayoutView {
        page: l.page.0,
        regions,
        report: l.report,
        algorithm: l.algorithm,
        manual: l.manual,
        revision: l.revision,
    }
}

#[tauri::command]
pub fn page_layout(state: State<'_, AppState>, index: u32) -> CmdResult<Option<PageLayoutView>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    Ok(p.store.page_layout(PageIndex(index))?.map(view))
}

/// Save a manual layout. Regions are validated, renumbered in the given
/// order, and marked manual so automatic reruns keep them (LAY-02). The
/// text pass for the page is invalidated (PIPE-01).
#[tauri::command]
pub fn layout_save(
    app: AppHandle,
    state: State<'_, AppState>,
    index: u32,
    regions: Vec<Region>,
) -> CmdResult<PageLayoutView> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    let page = PageIndex(index);
    let row = p
        .store
        .pages()?
        .into_iter()
        .find(|r| r.index == page)
        .ok_or_else(|| CommandError::new("not_found", format!("page {index} does not exist")))?;
    let (pw, ph) = (
        row.width_pt.unwrap_or(612.0),
        row.height_pt.unwrap_or(792.0),
    );
    let mut regions = regions;
    for r in &mut regions {
        if !(r.bbox.w > 1.0 && r.bbox.h > 1.0) {
            return Err(CommandError::new("invalid_input", "a region is empty"));
        }
        r.bbox.x = r.bbox.x.clamp(0.0, pw);
        r.bbox.y = r.bbox.y.clamp(0.0, ph);
        r.bbox.w = r.bbox.w.min(pw - r.bbox.x);
        r.bbox.h = r.bbox.h.min(ph - r.bbox.y);
        r.manual = true;
    }
    for (i, r) in regions.iter_mut().enumerate() {
        r.order = i as u32;
    }
    let existing_report = p
        .store
        .page_layout(page)?
        .map(|l| l.report)
        .unwrap_or(serde_json::Value::Null);
    p.store.put_page_layout(
        page,
        None,
        &serde_json::to_value(&regions)?,
        &existing_report,
        Some("manual"),
        true,
        true,
    )?;
    p.store.set_stage_done(page, Stage::Layout, true)?;
    p.store.set_stage_done(page, Stage::TextPass, false)?;
    p.store.add_history(
        "layout",
        &format!("Layout edited · page {}", page.number()),
        &serde_json::json!({ "page": index, "regions": regions.len() }),
    )?;
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    let saved = p
        .store
        .page_layout(page)?
        .ok_or_else(|| CommandError::new("other", "layout vanished after save"))?;
    Ok(view(saved))
}

/// Discard the manual layout and analyse the page again.
#[tauri::command]
pub fn layout_rerun(app: AppHandle, state: State<'_, AppState>, index: u32) -> CmdResult<()> {
    {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        let page = PageIndex(index);
        p.store
            .conn()
            .execute(
                "UPDATE page_layout SET manual = 0 WHERE page_index = ?1",
                rusqlite_params(index),
            )
            .map_err(|e| CommandError::new("other", e))?;
        p.store.set_stage_done(page, Stage::Layout, false)?;
        p.store.set_stage_done(page, Stage::TextPass, false)?;
        p.store.set_page_status(page, PageStatus::Queued, None)?;
    }
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    crate::commands::pipeline::start_pipeline(&app).map(|_| ())
}

fn rusqlite_params(index: u32) -> [i64; 1] {
    [index as i64]
}
