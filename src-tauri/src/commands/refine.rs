//! Targeted refinement commands (OCR-02, M8): region OCR with the
//! deterministic 300 DPI ×3 Lanczos profile, optional second engine, and
//! candidate merging into the review.

use sbwb_core::{PageIndex, Rect, RegionId, Stage};
use sbwb_ocr::OcrSettings;
use sbwb_store::MergeOutcome;
use sbwb_worker::{RequestKind, Response};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, CmdResult, CommandError};

#[derive(Debug, Clone, Serialize)]
pub struct RegionOcrResult {
    pub outcome: MergeOutcome,
    pub profile: serde_json::Value,
    pub render: Option<String>,
    pub run: String,
    pub elapsed_ms: u64,
    pub second_engine: Option<String>,
    pub raw_text: String,
    pub second_text: Option<String>,
}

#[tauri::command]
pub fn second_engine_available(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(sbwb_ocr::second::compiled() && sbwb_ocr::second::available(&state.resources.tessdata_dir))
}

/// Recognise a page area again: a 300 DPI crop enlarged exactly `enlarge`×,
/// the exact input saved as evidence, readings merged as candidates.
#[tauri::command]
pub fn region_ocr(
    app: AppHandle,
    state: State<'_, AppState>,
    index: u32,
    bbox: Rect,
    enlarge: Option<u32>,
    second_engine: Option<bool>,
    region: Option<String>,
) -> CmdResult<RegionOcrResult> {
    if !(bbox.w > 2.0 && bbox.h > 2.0) {
        return Err(CommandError::new(
            "invalid_input",
            "the area is too small to recognise",
        ));
    }
    let page = PageIndex(index);
    let enlarge = enlarge.unwrap_or(3).clamp(1, 4);
    let region = region.and_then(|r| RegionId::parse(&r));
    let (source, cache_dir, model, run) = {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        if p.store.is_read_only() {
            return Err(CommandError::new("conflict", "the book is open read-only"));
        }
        let row = p
            .store
            .pages()?
            .into_iter()
            .find(|r| r.index == page)
            .ok_or_else(|| CommandError::new("not_found", "page not found"))?;
        if !row.text_done {
            return Err(CommandError::new(
                "conflict",
                "process the page first; region OCR adds to its text",
            ));
        }
        if row.approval == "current" {
            return Err(CommandError::new(
                "conflict",
                "the page is approved; remove the approval before adding readings (PIPE-01)",
            ));
        }
        let settings = sbwb_pipeline::ProcessingSettings::load(&p.store)?;
        let run = p.store.start_run(
            Stage::Ocr,
            Some(page),
            Some("tesseract"),
            Some(settings.model.subdir()),
            &serde_json::json!({ "kind": "region", "bbox": bbox, "enlarge": enlarge, "dpi": 300, "second_engine": second_engine.unwrap_or(false) }),
        )?;
        (
            p.source_path.clone(),
            p.store.cache_dir(),
            settings.model,
            run,
        )
    };
    let render_path = cache_dir
        .join("ocr-renders")
        .join(format!("region-{run}.png"));
    let resp = state.with_worker(|w| {
        w.call(
            RequestKind::OcrRegion {
                path: source.clone(),
                password: None,
                page,
                crop: bbox,
                dpi: 300,
                enlarge,
                settings: OcrSettings {
                    model,
                    ..Default::default()
                },
                second_engine: second_engine.unwrap_or(false),
                save_render: Some(render_path.clone()),
            },
            |_| {},
        )
    });
    let (output, second, profile, elapsed_ms) = match resp {
        Ok(Response::RegionOcr {
            output,
            second,
            profile,
            elapsed_ms,
            ..
        }) => (output, second, profile, elapsed_ms),
        Ok(other) => {
            fail_run(&state, run, &format!("unexpected reply {other:?}"));
            return Err(CommandError::new("other", "unexpected worker reply"));
        }
        Err(e) => {
            fail_run(&state, run, &e.to_string());
            return Err(e.into());
        }
    };
    let raw_text: String = output
        .words
        .iter()
        .map(|w| w.text.as_str())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let second_text = second.as_ref().map(|s| {
        s.lines
            .iter()
            .map(|l| l.text.trim().to_string())
            .collect::<Vec<_>>()
            .join(" / ")
    });
    let outcome = {
        let mut guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_mut()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        let label = format!("region OCR at 300 dpi ×{enlarge}");
        let outcome = p.store.merge_region_candidates(
            page,
            run,
            region,
            &output.words,
            second.as_ref(),
            &label,
        )?;
        // Record the exact inputs with the run (IMG-01, OCR-02).
        let mut settings_json = profile.clone();
        settings_json["render"] =
            serde_json::Value::String(render_path.to_string_lossy().into_owned());
        settings_json["words"] = serde_json::to_value(&output.words)?;
        p.store.set_run_settings(run, &settings_json)?;
        p.store.finish_run(run, "ok", None, Some(elapsed_ms))?;
        p.store.add_history(
            "region_ocr",
            &format!(
                "Region OCR · page {} · {} words · {} candidates · {} inserted",
                page.number(),
                outcome.words,
                outcome.proposals_added,
                outcome.spans_inserted
            ),
            &serde_json::json!({ "page": index, "run": run, "bbox": bbox, "profile": profile, "outcome": outcome }),
        )?;
        outcome
    };
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    Ok(RegionOcrResult {
        outcome,
        profile,
        render: Some(render_path.to_string_lossy().into_owned()),
        run: run.to_string(),
        elapsed_ms,
        second_engine: second.map(|s| s.engine),
        raw_text,
        second_text,
    })
}

fn fail_run(state: &State<'_, AppState>, run: sbwb_core::RunId, error: &str) {
    if let Ok(guard) = state.project.lock() {
        if let Some(p) = guard.as_ref() {
            let _ = p.store.finish_run(run, "failed", Some(error), None);
        }
    }
}
