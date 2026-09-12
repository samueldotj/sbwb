//! Review commands (REV-01..06, PRJ-04, PRJ-05): issue inbox, navigation,
//! decisions, approvals, grouped corrections, history, drafts.

use sbwb_core::{HistoryId, IssueId, PageIndex, SpanId};
use sbwb_review::{GroupMatch, Issue, IssueCounts, IssueFilter};
use sbwb_store::{
    Decision, DecisionOutcome, Draft, GroupOutcome, HistoryEntry, NextIssue, PageIssueCounts,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::{AppState, CmdResult, CommandError};

fn with_project<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut sbwb_store::Project) -> CmdResult<T>,
) -> CmdResult<T> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_mut()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    f(&mut p.store)
}

fn changed(app: &AppHandle) {
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewCounts {
    pub book: IssueCounts,
    pub pages: Vec<PageIssueCounts>,
}

#[tauri::command]
pub fn review_counts(state: State<'_, AppState>, filter: IssueFilter) -> CmdResult<ReviewCounts> {
    with_project(&state, |p| {
        Ok(ReviewCounts {
            book: p.issue_counts(&filter)?,
            pages: p.page_issue_counts(&filter)?,
        })
    })
}

#[tauri::command]
pub fn page_issues(state: State<'_, AppState>, index: u32) -> CmdResult<Vec<Issue>> {
    with_project(&state, |p| Ok(p.issues_for_page(PageIndex(index))?))
}

#[tauri::command]
pub fn next_issue(
    state: State<'_, AppState>,
    from: Option<String>,
    from_page: Option<u32>,
    forward: bool,
    filter: IssueFilter,
) -> CmdResult<NextIssue> {
    let from = from.and_then(|s| IssueId::parse(&s));
    with_project(&state, |p| {
        Ok(p.next_issue(from, from_page.map(PageIndex), forward, &filter)?)
    })
}

#[tauri::command]
pub fn issue_decide(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    decision: Decision,
    span_revision: u64,
) -> CmdResult<DecisionOutcome> {
    let id =
        IssueId::parse(&id).ok_or_else(|| CommandError::new("invalid_input", "bad issue id"))?;
    let out = with_project(&state, |p| {
        Ok(p.decide_issue(id, &decision, span_revision)?)
    })?;
    changed(&app);
    Ok(out)
}

#[tauri::command]
pub fn span_flag(
    app: AppHandle,
    state: State<'_, AppState>,
    span: String,
    note: Option<String>,
) -> CmdResult<Issue> {
    let span =
        SpanId::parse(&span).ok_or_else(|| CommandError::new("invalid_input", "bad span id"))?;
    let out = with_project(&state, |p| Ok(p.flag_span(span, note.as_deref())?))?;
    changed(&app);
    Ok(out)
}

#[tauri::command]
pub fn flag_remove(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id =
        IssueId::parse(&id).ok_or_else(|| CommandError::new("invalid_input", "bad issue id"))?;
    with_project(&state, |p| Ok(p.remove_flag(id)?))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn page_approve(
    app: AppHandle,
    state: State<'_, AppState>,
    index: u32,
    acknowledged: u32,
) -> CmdResult<()> {
    with_project(&state, |p| {
        Ok(p.approve_page(PageIndex(index), acknowledged)?)
    })?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn page_unapprove(app: AppHandle, state: State<'_, AppState>, index: u32) -> CmdResult<()> {
    with_project(&state, |p| Ok(p.unapprove_page(PageIndex(index))?))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn group_preview(
    state: State<'_, AppState>,
    original: String,
    replacement: String,
) -> CmdResult<Vec<GroupMatch>> {
    with_project(&state, |p| {
        let scope = p.meta()?.scope;
        Ok(p.group_preview(&original, &replacement, &scope)?)
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupItem {
    pub span: String,
    pub revision: u64,
}

#[tauri::command]
pub fn group_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    original: String,
    replacement: String,
    items: Vec<GroupItem>,
) -> CmdResult<GroupOutcome> {
    let items: Vec<(SpanId, u64)> = items
        .iter()
        .filter_map(|i| SpanId::parse(&i.span).map(|s| (s, i.revision)))
        .collect();
    let out = with_project(&state, |p| {
        Ok(p.group_apply(&original, &replacement, &items)?)
    })?;
    changed(&app);
    Ok(out)
}

#[tauri::command]
pub fn history_list(
    state: State<'_, AppState>,
    limit: Option<u32>,
    span: Option<String>,
) -> CmdResult<Vec<HistoryEntry>> {
    with_project(&state, |p| match span.and_then(|s| SpanId::parse(&s)) {
        Some(s) => Ok(p.history_for_span(s)?),
        None => Ok(p.history(limit.unwrap_or(100))?),
    })
}

#[tauri::command]
pub fn history_undo(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = HistoryId::parse(&id)
        .ok_or_else(|| CommandError::new("invalid_input", "bad history id"))?;
    with_project(&state, |p| Ok(p.undo(id)?))?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn draft_put(state: State<'_, AppState>, span: String, text: String) -> CmdResult<()> {
    let span =
        SpanId::parse(&span).ok_or_else(|| CommandError::new("invalid_input", "bad span id"))?;
    with_project(&state, |p| Ok(p.put_draft(span, &text)?))
}

#[tauri::command]
pub fn draft_delete(state: State<'_, AppState>, span: String) -> CmdResult<()> {
    let span =
        SpanId::parse(&span).ok_or_else(|| CommandError::new("invalid_input", "bad span id"))?;
    with_project(&state, |p| Ok(p.delete_draft(span)?))
}

#[tauri::command]
pub fn draft_list(state: State<'_, AppState>, index: Option<u32>) -> CmdResult<Vec<Draft>> {
    with_project(&state, |p| Ok(p.drafts(index.map(PageIndex))?))
}

/// Per-book review preferences (threshold, filters) kept in the project
/// settings so they follow the file (REV-02).
#[tauri::command]
pub fn review_prefs_get(state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    with_project(&state, |p| {
        Ok(p.settings()?
            .get("review")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    })
}

#[tauri::command]
pub fn review_prefs_set(state: State<'_, AppState>, prefs: serde_json::Value) -> CmdResult<()> {
    with_project(&state, |p| {
        let mut all = p.settings()?;
        if !all.is_object() {
            all = serde_json::json!({});
        }
        all["review"] = prefs;
        Ok(p.set_settings(&all)?)
    })
}
