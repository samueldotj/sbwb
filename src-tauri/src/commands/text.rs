//! Text commands (TXT-02, TXT-03, PROV-01): effective text of a page,
//! proposals, book-wide text-pass counters, reruns, and vocabulary.

use sbwb_core::{PageIndex, Stage};
use sbwb_store::StoredProposal;
use sbwb_text::Span;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, CmdResult, CommandError};

#[derive(Debug, Clone, Serialize)]
pub struct PageTextView {
    pub page: u32,
    pub spans: Vec<Span>,
    pub proposals: Vec<StoredProposal>,
    pub text_revision: u64,
}

#[tauri::command]
pub fn page_text(state: State<'_, AppState>, index: u32) -> CmdResult<Option<PageTextView>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    let page = PageIndex(index);
    let spans = p.store.page_spans(page)?;
    if spans.is_empty() {
        return Ok(None);
    }
    let row = p.store.pages()?.into_iter().find(|r| r.index == page);
    Ok(Some(PageTextView {
        page: index,
        spans,
        proposals: p.store.page_proposals(page)?,
        text_revision: row.map(|r| r.text_revision).unwrap_or(0),
    }))
}

#[derive(Debug, Clone, Serialize)]
pub struct TextPassSummary {
    pub applied: u32,
    pub suggested: u32,
    pub accepted: u32,
    pub rejected: u32,
    pub deferred: u32,
    pub stale: u32,
    pub pages_done: u32,
    pub pages_in_scope: u32,
    pub last_run: Option<String>,
}

#[tauri::command]
pub fn text_pass_summary(state: State<'_, AppState>) -> CmdResult<Option<TextPassSummary>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    // No book open is a normal state for the status bar, not an error.
    let Some(p) = guard.as_ref() else {
        return Ok(None);
    };
    let counts = p.store.proposal_counts()?;
    let get = |k: &str| counts.get(k).copied().unwrap_or(0);
    let c = p.store.counts()?;
    let last_run: Option<String> = p
        .store
        .conn()
        .query_row(
            "SELECT finished_at FROM runs WHERE stage = 'text_pass' AND status = 'ok' ORDER BY finished_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    Ok(Some(TextPassSummary {
        applied: get("applied_auto"),
        suggested: get("open"),
        accepted: get("accepted"),
        rejected: get("rejected"),
        deferred: get("deferred"),
        stale: get("stale"),
        pages_done: c.text_done,
        pages_in_scope: c.in_scope,
        last_run,
    }))
}

/// Re-run the text pass: for one page or, with `None`, every page whose
/// layout is done. Approved pages are left alone (REV-06, PIPE-01).
#[tauri::command]
pub fn text_pass_rerun(
    app: AppHandle,
    state: State<'_, AppState>,
    index: Option<u32>,
) -> CmdResult<u32> {
    let mut n = 0;
    {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        for row in p.store.pages()? {
            if let Some(i) = index {
                if row.index.0 != i {
                    continue;
                }
            }
            if row.layout_done && row.approved_revision.is_none() {
                p.store.set_stage_done(row.index, Stage::TextPass, false)?;
                n += 1;
            }
        }
    }
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    if n > 0 {
        crate::commands::pipeline::start_pipeline(&app)?;
    }
    Ok(n)
}

#[tauri::command]
pub fn vocab_list(state: State<'_, AppState>) -> CmdResult<Vec<(String, String)>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    Ok(p.store.vocab()?)
}

#[tauri::command]
pub fn vocab_add(
    app: AppHandle,
    state: State<'_, AppState>,
    word: String,
    kind: String,
) -> CmdResult<()> {
    if word.trim().is_empty() {
        return Err(CommandError::new("invalid_input", "empty word"));
    }
    let kind = if kind == "protected" {
        "protected"
    } else {
        "vocab"
    };
    {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        p.store.add_vocab(&word, kind)?;
        p.store.add_history(
            "vocab",
            &format!(
                "{} “{}”",
                if kind == "protected" {
                    "Protected"
                } else {
                    "Added to vocabulary"
                },
                word.trim()
            ),
            &serde_json::json!({ "word": word.trim(), "kind": kind }),
        )?;
    }
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub fn vocab_remove(app: AppHandle, state: State<'_, AppState>, word: String) -> CmdResult<()> {
    {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        p.store.remove_vocab(&word)?;
    }
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    Ok(())
}
