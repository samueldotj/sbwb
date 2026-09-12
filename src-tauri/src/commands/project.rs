//! Project commands: inspect, import, open, close, scope, save a copy
//! (PRJ-01..05, UX-02, D-21).

use std::path::{Path, PathBuf};

use sbwb_core::{PageScope, SbwbError};
use sbwb_store::{LockState, OpenMode, Project, ProjectSummary, SourceInfo};
use sbwb_worker::{RequestKind, Response};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::render::{RenderCache, DEFAULT_CACHE_CAP};
use crate::state::{AppState, CmdResult, CommandError, OpenProject};

/// Refuse inputs above this size (PRJ-01 "excessively large").
const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Require this much free space beyond the PDF size for the project and cache.
const FREE_SPACE_MARGIN: u64 = 512 * 1024 * 1024;

pub const EVENT_PROJECT_CHANGED: &str = "project:changed";

#[derive(Debug, Clone, Serialize)]
pub struct SourceCheck {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub page_count: u32,
    pub encrypted: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    pub blake3: String,
    /// Approximate DPI of the first sampled page's scan, if embedded.
    pub scan_dpi: Option<f64>,
    pub has_text_layer: bool,
    pub suggested_project: PathBuf,
    pub free_space: Option<u64>,
    pub warnings: Vec<String>,
}

fn suggested_project_path(source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| "book".into());
    let mut name = stem;
    name.push(".sbwb");
    let mut candidate = source.with_file_name(&name);
    let mut n = 2;
    while candidate.exists() {
        let mut alt = source
            .file_stem()
            .map(|s| s.to_os_string())
            .unwrap_or_else(|| "book".into());
        alt.push(format!(" ({n}).sbwb"));
        candidate = source.with_file_name(alt);
        n += 1;
    }
    candidate
}

fn free_space_for(path: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    disks
        .iter()
        .filter(|d| {
            let mp = d.mount_point().to_string_lossy().to_string();
            let t = target.to_string_lossy().replace("\\\\?\\", "");
            t.to_ascii_lowercase().starts_with(&mp.to_ascii_lowercase())
        })
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

fn emit_changed(app: &AppHandle) {
    let _ = app.emit(EVENT_PROJECT_CHANGED, ());
}

/// Validate a PDF before import (PRJ-01). Runs inspection in the worker
/// process so a malformed file cannot take down the UI (NFR-11).
#[tauri::command]
pub fn inspect_source(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> CmdResult<SourceCheck> {
    let path = PathBuf::from(&path);
    let meta = std::fs::metadata(&path).map_err(|e| {
        CommandError::new("not_found", format!("cannot read {}: {e}", path.display()))
    })?;
    if !meta.is_file() {
        return Err(CommandError::new("invalid_input", "not a file"));
    }
    if meta.len() > MAX_SOURCE_BYTES {
        return Err(CommandError::new(
            "too_large",
            format!("{} is larger than the 2 GB limit", path.display()),
        ));
    }
    let info = match state.with_worker(|w| {
        w.call(
            RequestKind::InspectPdf {
                path: path.clone(),
                sample_pages: 3,
                password: password.clone(),
            },
            |_| {},
        )
    })? {
        Response::PdfInfo(i) => i,
        other => {
            return Err(CommandError::new(
                "other",
                format!("unexpected worker reply {other:?}"),
            ))
        }
    };
    let suggested = suggested_project_path(&path);
    let free = free_space_for(path.parent().unwrap_or(&path));
    let mut warnings = Vec::new();
    if let Some(f) = free {
        if f < meta.len() + FREE_SPACE_MARGIN {
            warnings.push(format!(
                "Only {} MB free next to the PDF; the project needs about {} MB.",
                f / 1_048_576,
                (meta.len() + FREE_SPACE_MARGIN) / 1_048_576
            ));
        }
    }
    let first = info.sample_pages.first();
    if let Some(p) = first {
        if let Some(dpi) = p.image_dpi {
            if dpi < 200.0 {
                warnings.push(format!("The embedded scan is about {dpi:.0} DPI; recognition quality may suffer below 300 DPI."));
            }
        }
    }
    if info.encrypted {
        warnings.push("The PDF is password protected.".into());
    }
    Ok(SourceCheck {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path,
        size: meta.len(),
        page_count: info.page_count,
        encrypted: info.encrypted,
        title: info.title,
        author: info.author,
        blake3: info.blake3,
        scan_dpi: first.and_then(|p| p.image_dpi),
        has_text_layer: first.map(|p| p.has_text).unwrap_or(false),
        suggested_project: suggested,
        free_space: free,
        warnings,
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportRequest {
    pub source: String,
    /// Where to create the project; defaults next to the PDF (D-21).
    pub project_path: Option<String>,
    pub password: Option<String>,
    /// Trial scope: first N pages (D-21 default 50). `None` means all pages.
    pub first_pages: Option<u32>,
}

/// Create a project from a PDF and open it (PRJ-01, PRJ-02).
#[tauri::command]
pub fn import_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    req: ImportRequest,
) -> CmdResult<ProjectSummary> {
    if state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?
        .is_some()
    {
        return Err(CommandError::new(
            "conflict",
            "close the current book before importing another",
        ));
    }
    let check = inspect_source(state.clone(), req.source.clone(), req.password.clone())?;
    if check.encrypted {
        return Err(CommandError::new(
            "needs_password",
            "this PDF needs a password",
        ));
    }
    if check.page_count == 0 {
        return Err(CommandError::new("invalid_input", "the PDF has no pages"));
    }
    let project_path = req
        .project_path
        .map(PathBuf::from)
        .unwrap_or(check.suggested_project.clone());
    if let Some(free) = free_space_for(project_path.parent().unwrap_or(&project_path)) {
        if free < check.size + FREE_SPACE_MARGIN {
            return Err(CommandError::new(
                "disk_full",
                format!(
                    "not enough free space at {} for the project",
                    project_path.display()
                ),
            ));
        }
    }
    let sizes = match state.with_worker(|w| {
        w.call(
            RequestKind::PageSizes {
                path: check.path.clone(),
                password: req.password.clone(),
            },
            |_| {},
        )
    })? {
        Response::PageSizes { sizes } => sizes,
        other => {
            return Err(CommandError::new(
                "other",
                format!("unexpected worker reply {other:?}"),
            ))
        }
    };
    let scope = match req.first_pages {
        Some(n) => PageScope::first_n(n, check.page_count),
        None => PageScope::all(check.page_count),
    };
    let source = SourceInfo {
        name: check.name.clone(),
        size: check.size,
        blake3: check.blake3.clone(),
        page_count: check.page_count,
        title: check.title.clone(),
        author: check.author.clone(),
    };
    let store = Project::create(
        &project_path,
        &check.path,
        source,
        &sizes,
        scope,
        env!("CARGO_PKG_VERSION"),
    )?;
    let source_path = store.source_path()?;
    let render_cache =
        std::sync::Arc::new(RenderCache::new(&store.cache_dir(), DEFAULT_CACHE_CAP)?);
    let summary = store.summary()?;
    tracing::info!(project = %summary.path.display(), source = %source_path.display(), pages = summary.meta.source.page_count, "imported");
    *state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))? = Some(OpenProject {
        store,
        source_path,
        render_cache,
    });
    emit_changed(&app);
    // UX-02: processing starts right after import when the book's settings
    // say so (default on). Opening never starts anything (PIPE-02).
    let auto = sbwb_pipeline::ProcessingSettings::default().run_stages_automatically;
    if auto {
        if let Err(e) = crate::commands::pipeline::start_pipeline(&app) {
            tracing::warn!("auto-start after import failed: {}", e.message);
        }
    }
    Ok(summary)
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenResult {
    pub summary: ProjectSummary,
    /// Set when the project was opened read-only because another instance
    /// holds the writer lock (PRJ-02).
    pub locked_by: Option<String>,
}

/// Open a project; falls back to read-only when another writer is live.
/// Never starts processing or network activity (PIPE-02).
#[tauri::command]
pub fn open_project(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<OpenResult> {
    if state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?
        .is_some()
    {
        return Err(CommandError::new(
            "conflict",
            "close the current book before opening another",
        ));
    }
    let path = PathBuf::from(path);
    let (store, locked_by) = match Project::open(&path, OpenMode::ReadWrite) {
        Ok(p) => (p, None),
        Err(SbwbError::Conflict(msg)) => {
            let who = match Project::peek_lock(&path) {
                Ok(LockState::Other(l)) => format!("process {} on {}", l.pid, l.host),
                _ => msg,
            };
            (Project::open(&path, OpenMode::ReadOnly)?, Some(who))
        }
        Err(e) => return Err(e.into()),
    };
    store.verify_source()?;
    let source_path = store.source_path()?;
    let render_cache =
        std::sync::Arc::new(RenderCache::new(&store.cache_dir(), DEFAULT_CACHE_CAP)?);
    let summary = store.summary()?;
    tracing::info!(project = %summary.path.display(), source = %source_path.display(), read_only = summary.read_only, "opened");
    *state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))? = Some(OpenProject {
        store,
        source_path,
        render_cache,
    });
    emit_changed(&app);
    Ok(OpenResult { summary, locked_by })
}

#[tauri::command]
pub fn project_summary(state: State<'_, AppState>) -> CmdResult<Option<ProjectSummary>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    match guard.as_ref() {
        Some(p) => Ok(Some(p.store.summary()?)),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn close_project(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    crate::commands::pipeline::stop_for_close(&app);
    let taken = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?
        .take();
    if let Some(p) = taken {
        p.store.close()?;
    }
    emit_changed(&app);
    Ok(())
}

/// Write a consistent portable copy (PRJ-02).
#[tauri::command]
pub fn save_copy(state: State<'_, AppState>, dest: String) -> CmdResult<()> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    p.store.save_copy(Path::new(&dest))?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScopeRequest {
    /// Inclusive 1-based page ranges, e.g. [[1,50]]. Empty means all pages.
    pub ranges: Vec<(u32, u32)>,
}

/// Change the processing scope without re-importing (PRJ-03, A-02).
#[tauri::command]
pub fn set_scope(
    app: AppHandle,
    state: State<'_, AppState>,
    req: ScopeRequest,
) -> CmdResult<ProjectSummary> {
    let mut guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_mut()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    let total = p.store.meta()?.source.page_count;
    let scope = if req.ranges.is_empty() {
        PageScope::all(total)
    } else {
        PageScope::from_ranges(req.ranges, total)
    };
    if scope.is_empty() {
        return Err(CommandError::new(
            "invalid_input",
            "the page range is empty",
        ));
    }
    p.store.set_scope(&scope)?;
    p.store.add_history(
        "scope",
        &format!("Pages to process: {}", scope.label()),
        &serde_json::json!({ "scope": scope }),
    )?;
    let summary = p.store.summary()?;
    emit_changed(&app);
    Ok(summary)
}

/// Heartbeat the writer lock; called from a background thread.
pub fn heartbeat(state: &AppState) {
    if let Ok(guard) = state.project.lock() {
        if let Some(p) = guard.as_ref() {
            if let Err(e) = p.store.heartbeat() {
                tracing::warn!("lock heartbeat failed: {e}");
            }
        }
    }
}
