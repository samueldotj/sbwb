//! Export commands (EXP-01..06): readiness, preview of exclusions and
//! warnings, the export run on a background thread with progress events,
//! cancel, previous exports, and per-book export defaults.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sbwb_core::{PageIndex, Rect, Result as CoreResult, SbwbError};
use sbwb_export::snapshot::{BookInfo, FlagSnap, PageSnap, RegionSnap, SpanSnap};
use sbwb_export::{ExportReport, ExportSettings, ExportSnapshot, Phase, RenderProvider};
use sbwb_review::IssueFilter;
use sbwb_store::{ExportRecord, Project};
use sbwb_worker::{RequestKind, Response, Worker};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, CmdResult, CommandError};

pub const EVENT_EXPORT: &str = "export:event";

#[derive(Default)]
pub struct ExportSlot {
    pub running: Mutex<bool>,
    pub cancel: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ExportEvent {
    Progress { phase: Phase, done: u32, total: u32 },
    Finished { report: ExportReport },
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Readiness {
    pub pages_in_scope: u32,
    pub pages_indexed: u32,
    pub pages_approved: u32,
    pub pages_left: u32,
    pub unresolved_below_threshold: u32,
    pub deferred: u32,
    pub ai_pending: u32,
    pub clean_ready: bool,
    pub previous: Vec<ExportRecord>,
    pub default_name: String,
    pub book_title: Option<String>,
    pub book_author: Option<String>,
    pub pdf_title: Option<String>,
    pub pdf_author: Option<String>,
}

fn with_project<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Project) -> CmdResult<T>,
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

/// Copy everything the export needs out of the project (EXP-04).
fn build_snapshot(
    store: &Project,
    settings: ExportSettings,
    threshold: u8,
) -> CoreResult<ExportSnapshot> {
    let meta = store.meta()?;
    let mut pages = Vec::new();
    for row in store.pages()? {
        if !meta.scope.contains(row.index) || !row.text_done {
            continue;
        }
        let spans = store.page_spans(row.index)?;
        let regions: Vec<sbwb_layout::Region> = store
            .page_layout(row.index)?
            .map(|l| serde_json::from_value(l.regions).unwrap_or_default())
            .unwrap_or_default();
        let issues = store.issues_for_page(row.index)?;
        let mut unresolved = 0;
        let mut deferred = 0;
        let flags: std::collections::HashMap<_, FlagSnap> = issues
            .iter()
            .filter(|i| {
                matches!(
                    i.status,
                    sbwb_review::IssueStatus::Open | sbwb_review::IssueStatus::Deferred
                )
            })
            .map(|i| {
                if i.status == sbwb_review::IssueStatus::Deferred {
                    deferred += 1;
                } else {
                    unresolved += 1;
                }
                (
                    i.span,
                    FlagSnap {
                        kind: i.kind.as_str().to_string(),
                        score: i.score,
                        reason: i.reason.clone(),
                        deferred: i.status == sbwb_review::IssueStatus::Deferred,
                    },
                )
            })
            .collect();
        let page = PageSnap {
            index: row.index.0,
            label: row.printed_label.clone(),
            width_pt: row.width_pt.unwrap_or(612.0),
            height_pt: row.height_pt.unwrap_or(792.0),
            approval: row.approval.clone(),
            approved_outstanding: row.approved_outstanding,
            unresolved,
            deferred,
            text_done: row.text_done,
            regions: regions
                .iter()
                .map(|r| RegionSnap {
                    id: r.id,
                    kind: r.kind,
                    order: r.order,
                    structure: r.structure,
                    bbox: r.bbox,
                    anchor_y: r.anchor_y,
                })
                .collect(),
            spans: spans
                .iter()
                .map(|s| SpanSnap {
                    id: s.id,
                    region: s.region,
                    text: s.text.clone(),
                    trailing: s.trailing.clone(),
                    origin: s.origin,
                    confidence: s.confidence,
                    paragraph_start: s.paragraph_start,
                    structure: s.structure,
                    anchors: s.anchors.iter().map(|a| (a.page.0, a.bbox)).collect(),
                    flag: flags.get(&s.id).cloned(),
                })
                .collect(),
        };
        pages.push(page);
    }
    let _ = threshold;
    Ok(ExportSnapshot {
        id: sbwb_core::ExportId::new().to_string(),
        created_at: jiff::Timestamp::now().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        book: BookInfo {
            title: meta.title.clone().or_else(|| meta.source.title.clone()),
            author: meta.author.clone().or_else(|| meta.source.author.clone()),
            source_name: meta.source.name.clone(),
            source_pages: meta.source.page_count,
            source_blake3: meta.source.blake3.clone(),
            pdf_title: meta.source.title.clone(),
            pdf_author: meta.source.author.clone(),
        },
        settings,
        pages,
    })
}

/// Illustrations come from the render worker as PNG crops.
struct WorkerRenders {
    worker: Mutex<Worker>,
    source: PathBuf,
    tmp: PathBuf,
}

impl RenderProvider for WorkerRenders {
    fn crop_png(&self, page: u32, bbox: &Rect, dpi: u32) -> CoreResult<Vec<u8>> {
        let mut w = self
            .worker
            .lock()
            .map_err(|_| SbwbError::other("render worker poisoned"))?;
        let out = self.tmp.join(format!(
            "export-crop-{page}-{}.png",
            jiff::Timestamp::now().as_millisecond()
        ));
        let resp = w.call(
            RequestKind::RenderPage {
                path: self.source.clone(),
                password: None,
                page: PageIndex(page),
                scale: dpi as f64 / 72.0,
                crop: Some(*bbox),
                max_dimension: 6000,
                out: out.clone(),
            },
            |_| {},
        )?;
        match resp {
            Response::Rendered { .. } => {
                let bytes = std::fs::read(&out)?;
                let _ = std::fs::remove_file(&out);
                Ok(bytes)
            }
            other => Err(SbwbError::other(format!(
                "unexpected render reply {other:?}"
            ))),
        }
    }
}

fn review_threshold(store: &Project) -> u8 {
    store
        .settings()
        .ok()
        .and_then(|s| {
            s.get("review")
                .and_then(|r| r.get("threshold"))
                .and_then(|t| t.as_u64())
        })
        .map(|t| t as u8)
        .unwrap_or(70)
}

#[tauri::command]
pub fn export_readiness(state: State<'_, AppState>) -> CmdResult<Readiness> {
    with_project(&state, |p| {
        let meta = p.meta()?;
        let pages = p.pages()?;
        let in_scope: Vec<_> = pages
            .iter()
            .filter(|r| meta.scope.contains(r.index))
            .collect();
        let approved = in_scope.iter().filter(|r| r.approval == "current").count() as u32;
        let indexed = in_scope.iter().filter(|r| r.text_done).count() as u32;
        let counts = p.issue_counts(&IssueFilter {
            threshold: review_threshold(p),
            ..Default::default()
        })?;
        let stem = meta
            .title
            .clone()
            .or(meta.source.title.clone())
            .unwrap_or_else(|| meta.source.name.trim_end_matches(".pdf").to_string());
        let safe: String = stem
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Readiness {
            pages_in_scope: meta.scope.len(),
            pages_indexed: indexed,
            pages_approved: approved,
            pages_left: meta.scope.len().saturating_sub(approved),
            unresolved_below_threshold: counts.matching,
            deferred: counts.deferred,
            ai_pending: 0,
            clean_ready: approved == meta.scope.len() && !meta.scope.is_empty(),
            previous: p.exports()?,
            default_name: format!(
                "{}.docx",
                if safe.is_empty() {
                    "book".to_string()
                } else {
                    safe
                }
            ),
            book_title: meta.title.clone().or(meta.source.title.clone()),
            book_author: meta.author.clone().or(meta.source.author.clone()),
            pdf_title: meta.source.title,
            pdf_author: meta.source.author,
        })
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportPreview {
    pub explanation: String,
    pub exclusions: Vec<sbwb_export::Exclusion>,
    pub excluded_words: u32,
    pub warnings: Vec<String>,
    pub stats: sbwb_export::PlanStats,
    pub clean_blockers: Vec<u32>,
}

#[tauri::command]
pub fn export_preview(
    state: State<'_, AppState>,
    settings: ExportSettings,
) -> CmdResult<ExportPreview> {
    with_project(&state, |p| {
        let threshold = review_threshold(p);
        let snapshot = build_snapshot(p, settings, threshold)?;
        let plan = sbwb_export::compose(&snapshot);
        let mut warnings = plan.warnings.clone();
        if let Some(w) = sbwb_export::fonts::warning_for(&snapshot.settings.preset.body_font) {
            warnings.push(w);
        }
        Ok(ExportPreview {
            explanation: snapshot.settings.furniture_explanation(),
            excluded_words: plan.exclusions.iter().map(|e| e.words).sum(),
            exclusions: plan.exclusions,
            warnings,
            stats: plan.stats,
            clean_blockers: sbwb_export::clean_copy_blockers(&snapshot),
        })
    })
}

/// Start an export on a background thread. Progress and the outcome arrive
/// as `export:event`; the project file records the export on success.
#[tauri::command]
pub fn export_run(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: ExportSettings,
    dest: String,
) -> CmdResult<()> {
    let slot = app.state::<ExportSlot>();
    {
        let mut running = slot
            .running
            .lock()
            .map_err(|_| CommandError::new("other", "export mutex poisoned"))?;
        if *running {
            return Err(CommandError::new(
                "conflict",
                "an export is already running",
            ));
        }
        *running = true;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *slot
        .cancel
        .lock()
        .map_err(|_| CommandError::new("other", "export mutex poisoned"))? = Some(cancel.clone());
    let dest = PathBuf::from(dest);
    // Snapshot and a dedicated render worker are taken while the project lock is held.
    let (snapshot, source, cache_dir) = {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        let threshold = review_threshold(&p.store);
        (
            build_snapshot(&p.store, settings, threshold)?,
            p.source_path.clone(),
            p.store.cache_dir(),
        )
    };
    let worker = Worker::spawn(state.worker_config()).ok();
    let handle = app.clone();
    std::thread::Builder::new()
        .name("sbwb-export".into())
        .spawn(move || {
            let renders: Box<dyn RenderProvider> = match worker {
                Some(w) => Box::new(WorkerRenders { worker: Mutex::new(w), source, tmp: cache_dir }),
                None => Box::new(sbwb_export::NoRenders),
            };
            let mut progress = |phase: Phase, done: u32, total: u32| {
                let _ = handle.emit(EVENT_EXPORT, ExportEvent::Progress { phase, done, total });
            };
            let result = sbwb_export::run(&snapshot, &dest, renders.as_ref(), &cancel, &mut progress);
            match result {
                Ok(report) => {
                    if let Some(st) = handle.try_state::<AppState>() {
                        if let Ok(guard) = st.project.lock() {
                            if let Some(p) = guard.as_ref() {
                                let kind = format!("{:?}", report.copy).to_lowercase();
                                let _ = p.store.record_export(
                                    &report.id,
                                    &kind,
                                    &report.docx_path,
                                    Some(&report.checksum_blake3),
                                    &serde_json::to_value(&report.settings).unwrap_or_default(),
                                    &serde_json::to_value(&report).unwrap_or_default(),
                                );
                                let _ = p.store.add_history(
                                    "export",
                                    &format!("Exported {} copy · {} pages · {}", kind, report.stats.pages, report.docx_path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
                                    &serde_json::json!({ "export": report.id, "checksum": report.checksum_blake3, "path": report.docx_path }),
                                );
                            }
                        }
                    }
                    let _ = handle.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
                    let _ = handle.emit(EVENT_EXPORT, ExportEvent::Finished { report });
                }
                Err(e) => {
                    let _ = handle.emit(EVENT_EXPORT, ExportEvent::Failed { message: e.to_string() });
                }
            }
            if let Some(slot) = handle.try_state::<ExportSlot>() {
                if let Ok(mut r) = slot.running.lock() {
                    *r = false;
                }
            }
        })
        .map_err(|e| CommandError::new("other", format!("export thread: {e}")))?;
    Ok(())
}

#[tauri::command]
pub fn export_cancel(app: AppHandle) -> CmdResult<()> {
    let slot = app.state::<ExportSlot>();
    if let Some(c) = slot
        .cancel
        .lock()
        .map_err(|_| CommandError::new("other", "export mutex poisoned"))?
        .as_ref()
    {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub fn export_defaults_get(state: State<'_, AppState>) -> CmdResult<ExportSettings> {
    with_project(&state, |p| {
        Ok(p.settings()?
            .get("export")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    })
}

#[tauri::command]
pub fn export_defaults_set(state: State<'_, AppState>, settings: ExportSettings) -> CmdResult<()> {
    with_project(&state, |p| {
        let mut all = p.settings()?;
        if !all.is_object() {
            all = serde_json::json!({});
        }
        all["export"] = serde_json::to_value(&settings)?;
        Ok(p.set_settings(&all)?)
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportEntry {
    pub id: String,
    pub ts: String,
    pub kind: String,
    pub path: PathBuf,
    pub name: String,
    pub exists: bool,
    pub pages: u32,
    pub archive: Option<PathBuf>,
}

/// Previous exports of the open book, newest first, with whether the file
/// is still there (the rail shows them as clickable documents).
#[tauri::command]
pub fn exports_list(state: State<'_, AppState>) -> CmdResult<Vec<ExportEntry>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let Some(p) = guard.as_ref() else {
        return Ok(vec![]);
    };
    // Newest first; one entry per path however it was spelled.
    let mut seen = std::collections::HashSet::new();
    Ok(p.store
        .exports()?
        .into_iter()
        .filter(|e| seen.insert(e.path.to_string_lossy().replace('\\', "/").to_lowercase()))
        .map(|e| {
            let pages = e
                .report
                .as_ref()
                .and_then(|r| r.pointer("/stats/pages"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let archive = e
                .report
                .as_ref()
                .and_then(|r| r.get("archive_path"))
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .filter(|a| a.is_file());
            ExportEntry {
                name: e
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                exists: e.path.is_file(),
                id: e.id,
                ts: e.ts,
                kind: e.kind,
                path: e.path,
                pages,
                archive,
            }
        })
        .collect())
}

/// Open a file with its default application (Word for `.docx`).
#[tauri::command]
pub fn open_file(path: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(CommandError::new(
            "not_found",
            format!("{} is no longer there", p.display()),
        ));
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &p.to_string_lossy()])
            .spawn()
            .map_err(|e| CommandError::new("other", format!("open file: {e}")))?;
    }
    #[cfg(not(windows))]
    {
        let _ = p;
    }
    Ok(())
}

#[tauri::command]
pub fn open_path(path: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    let target = if p.is_file() {
        p.parent().map(|d| d.to_path_buf()).unwrap_or(p)
    } else {
        p
    };
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&target)
            .spawn()
            .map_err(|e| CommandError::new("other", format!("open folder: {e}")))?;
    }
    #[cfg(not(windows))]
    {
        let _ = target;
    }
    Ok(())
}
