//! Multiple-book queue (PRJ-06): several PDFs with an explicit profile and
//! destination, one independent project each, processed one after another
//! on a background thread. The queue is persisted in the app data folder;
//! after a restart it resumes only on request.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sbwb_core::{PageScope, SbwbError};
use sbwb_pipeline::{PipelineEvent, ProcessingSettings, Scheduler};
use sbwb_store::{OpenMode, Project, SourceInfo};
use sbwb_worker::{RequestKind, Response, Worker};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, CmdResult, CommandError};

pub const EVENT_QUEUE: &str = "queue:event";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Pending,
    Running,
    Done,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub source: PathBuf,
    pub project_path: PathBuf,
    pub first_pages: Option<u32>,
    pub settings: ProcessingSettings,
    pub status: ItemStatus,
    pub error: Option<String>,
    pub done: u32,
    pub total: u32,
    pub added_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueFile {
    pub items: Vec<QueueItem>,
}

#[derive(Default)]
pub struct QueueSlot {
    pub items: Mutex<Vec<QueueItem>>,
    pub running: AtomicBool,
    pub stop: Mutex<Option<Arc<AtomicBool>>>,
}

fn queue_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("queue.json")
}

pub fn load(app: &AppHandle) {
    let path = queue_path(app);
    let slot = app.state::<QueueSlot>();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(mut q) = serde_json::from_str::<QueueFile>(&text) {
            // A run in progress when the app closed is not resumed silently.
            for it in &mut q.items {
                if it.status == ItemStatus::Running {
                    it.status = ItemStatus::Interrupted;
                }
            }
            if let Ok(mut items) = slot.items.lock() {
                *items = q.items;
            }
        }
    }
}

fn save(app: &AppHandle) {
    let slot = app.state::<QueueSlot>();
    let guard = slot.items.lock();
    if let Ok(items) = guard {
        let path = queue_path(app);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&QueueFile {
                items: items.clone(),
            })
            .unwrap_or_default(),
        );
    }
}

fn emit(app: &AppHandle) {
    let slot = app.state::<QueueSlot>();
    let guard = slot.items.lock();
    if let Ok(items) = guard {
        let _ = app.emit(
            EVENT_QUEUE,
            QueueView {
                items: items.clone(),
                running: slot.running.load(Ordering::SeqCst),
            },
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueView {
    pub items: Vec<QueueItem>,
    pub running: bool,
}

#[tauri::command]
pub fn queue_list(app: AppHandle) -> CmdResult<QueueView> {
    let slot = app.state::<QueueSlot>();
    let items = slot
        .items
        .lock()
        .map_err(|_| CommandError::new("other", "queue poisoned"))?
        .clone();
    Ok(QueueView {
        items,
        running: slot.running.load(Ordering::SeqCst),
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueAdd {
    pub sources: Vec<String>,
    /// Folder for the project files; `None` puts each next to its PDF.
    pub dest_dir: Option<String>,
    pub first_pages: Option<u32>,
    pub settings: ProcessingSettings,
}

#[tauri::command]
pub fn queue_add(app: AppHandle, req: QueueAdd) -> CmdResult<QueueView> {
    let slot = app.state::<QueueSlot>();
    {
        let mut items = slot
            .items
            .lock()
            .map_err(|_| CommandError::new("other", "queue poisoned"))?;
        for s in req.sources {
            let source = PathBuf::from(&s);
            if !source.is_file() {
                return Err(CommandError::new("not_found", format!("{s} is not a file")));
            }
            let stem = source
                .file_stem()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| "book".into());
            let dir =
                req.dest_dir.as_ref().map(PathBuf::from).unwrap_or_else(|| {
                    source.parent().map(|p| p.to_path_buf()).unwrap_or_default()
                });
            let mut project_path = dir.join(format!("{stem}.sbwb"));
            let mut n = 2;
            while project_path.exists() || items.iter().any(|i| i.project_path == project_path) {
                project_path = dir.join(format!("{stem}-{n}.sbwb"));
                n += 1;
            }
            items.push(QueueItem {
                id: sbwb_core::JobId::new().to_string(),
                source,
                project_path,
                first_pages: req.first_pages,
                settings: req.settings.clone(),
                status: ItemStatus::Pending,
                error: None,
                done: 0,
                total: 0,
                added_at: jiff::Timestamp::now().to_string(),
            });
        }
    }
    save(&app);
    emit(&app);
    queue_list(app)
}

#[tauri::command]
pub fn queue_remove(app: AppHandle, id: String) -> CmdResult<QueueView> {
    let slot = app.state::<QueueSlot>();
    {
        let mut items = slot
            .items
            .lock()
            .map_err(|_| CommandError::new("other", "queue poisoned"))?;
        if items
            .iter()
            .any(|i| i.id == id && i.status == ItemStatus::Running)
        {
            return Err(CommandError::new(
                "conflict",
                "stop the queue before removing the running book",
            ));
        }
        // Removing never deletes the project (PRJ-06).
        items.retain(|i| i.id != id);
    }
    save(&app);
    emit(&app);
    queue_list(app)
}

#[tauri::command]
pub fn queue_move(app: AppHandle, id: String, delta: i32) -> CmdResult<QueueView> {
    let slot = app.state::<QueueSlot>();
    {
        let mut items = slot
            .items
            .lock()
            .map_err(|_| CommandError::new("other", "queue poisoned"))?;
        if let Some(i) = items.iter().position(|it| it.id == id) {
            let j = (i as i32 + delta).clamp(0, items.len() as i32 - 1) as usize;
            if items[i].status == ItemStatus::Pending && items[j].status == ItemStatus::Pending {
                items.swap(i, j);
            }
        }
    }
    save(&app);
    emit(&app);
    queue_list(app)
}

#[tauri::command]
pub fn queue_retry(app: AppHandle, id: String) -> CmdResult<QueueView> {
    let slot = app.state::<QueueSlot>();
    if let Ok(mut items) = slot.items.lock() {
        if let Some(it) = items.iter_mut().find(|i| i.id == id) {
            if matches!(it.status, ItemStatus::Failed | ItemStatus::Interrupted) {
                it.status = ItemStatus::Pending;
                it.error = None;
            }
        }
    }
    save(&app);
    emit(&app);
    queue_list(app)
}

#[tauri::command]
pub fn queue_clear_finished(app: AppHandle) -> CmdResult<QueueView> {
    let slot = app.state::<QueueSlot>();
    if let Ok(mut items) = slot.items.lock() {
        items.retain(|i| !matches!(i.status, ItemStatus::Done));
    }
    save(&app);
    emit(&app);
    queue_list(app)
}

#[tauri::command]
pub fn queue_stop(app: AppHandle) -> CmdResult<()> {
    let slot = app.state::<QueueSlot>();
    if let Some(s) = slot
        .stop
        .lock()
        .map_err(|_| CommandError::new("other", "queue poisoned"))?
        .as_ref()
    {
        s.store(true, Ordering::SeqCst);
    }
    Ok(())
}

pub fn is_running(app: &AppHandle) -> bool {
    app.state::<QueueSlot>().running.load(Ordering::SeqCst)
}

/// Create the project for a queued PDF without opening it in the UI.
fn create_project(
    worker_cfg: sbwb_worker::WorkerConfig,
    item: &QueueItem,
) -> sbwb_core::Result<(PathBuf, u32)> {
    let info = sbwb_pdf::inspect_file(&item.source, 3, None)?;
    if info.encrypted {
        return Err(SbwbError::InvalidInput(
            "the PDF needs a password; import it from the Welcome page instead".into(),
        ));
    }
    let bytes = std::fs::read(&item.source)?;
    let mut worker = Worker::spawn(worker_cfg)?;
    let sizes = match worker.call(
        RequestKind::PageSizes {
            path: item.source.clone(),
            password: None,
        },
        |_| {},
    )? {
        Response::PageSizes { sizes } => sizes,
        other => {
            return Err(SbwbError::other(format!(
                "unexpected worker reply {other:?}"
            )))
        }
    };
    worker.shutdown();
    let scope = match item.first_pages {
        Some(n) => PageScope::first_n(n, info.page_count),
        None => PageScope::all(info.page_count),
    };
    let source = SourceInfo {
        name: item
            .source
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: bytes.len() as u64,
        blake3: blake3::hash(&bytes).to_hex().to_string(),
        page_count: info.page_count,
        title: info.title.clone(),
        author: info.author.clone(),
    };
    let store = Project::create(
        &item.project_path,
        &item.source,
        source,
        &sizes,
        scope.clone(),
        env!("CARGO_PKG_VERSION"),
    )?;
    item.settings.save(&store)?;
    let src = store.source_path()?;
    store.close()?;
    Ok((src, scope.len()))
}

/// Process pending books in order; one pipeline at a time, worker caps
/// apply as for interactive processing (PRJ-06 resource limits).
#[tauri::command]
pub fn queue_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let slot = app.state::<QueueSlot>();
    if slot.running.swap(true, Ordering::SeqCst) {
        return Err(CommandError::new(
            "conflict",
            "the queue is already running",
        ));
    }
    if crate::commands::pipeline::is_active(&app) {
        slot.running.store(false, Ordering::SeqCst);
        return Err(CommandError::new(
            "conflict",
            "processing of the open book is running; wait for it or pause it first",
        ));
    }
    let stop = Arc::new(AtomicBool::new(false));
    *slot
        .stop
        .lock()
        .map_err(|_| CommandError::new("other", "queue poisoned"))? = Some(stop.clone());
    let worker_cfg = state.worker_config();
    let lexicon = sbwb_text::LexiconPaths::in_dir(&state.resources.lexicon_dir);
    let handle = app.clone();
    std::thread::Builder::new()
        .name("sbwb-queue".into())
        .spawn(move || {
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                // next pending item
                let next = {
                    let slot = handle.state::<QueueSlot>();
                    let mut items = match slot.items.lock() {
                        Ok(i) => i,
                        Err(_) => break,
                    };
                    match items.iter_mut().find(|i| i.status == ItemStatus::Pending) {
                        Some(it) => {
                            it.status = ItemStatus::Running;
                            it.error = None;
                            Some(it.clone())
                        }
                        None => None,
                    }
                };
                let Some(item) = next else { break };
                save(&handle);
                emit(&handle);
                let outcome = run_item(&handle, &item, worker_cfg.clone(), &lexicon, &stop);
                {
                    let slot = handle.state::<QueueSlot>();
                    let guard = slot.items.lock();
                    if let Ok(mut items) = guard {
                        if let Some(it) = items.iter_mut().find(|i| i.id == item.id) {
                            match &outcome {
                                Ok((done, total, cancelled)) => {
                                    it.done = *done;
                                    it.total = *total;
                                    it.status = if *cancelled {
                                        ItemStatus::Interrupted
                                    } else {
                                        ItemStatus::Done
                                    };
                                }
                                Err(e) => {
                                    it.status = ItemStatus::Failed;
                                    it.error = Some(e.to_string());
                                }
                            }
                        }
                    }
                }
                save(&handle);
                emit(&handle);
                if outcome.as_ref().map(|o| o.2).unwrap_or(false) {
                    break;
                }
            }
            handle
                .state::<QueueSlot>()
                .running
                .store(false, Ordering::SeqCst);
            emit(&handle);
        })
        .map_err(|e| CommandError::new("other", format!("queue thread: {e}")))?;
    emit(&app);
    Ok(())
}

fn run_item(
    app: &AppHandle,
    item: &QueueItem,
    worker_cfg: sbwb_worker::WorkerConfig,
    lexicon: &sbwb_text::LexiconPaths,
    stop: &AtomicBool,
) -> sbwb_core::Result<(u32, u32, bool)> {
    let (src, total) = if item.project_path.exists() {
        let p = Project::open(&item.project_path, OpenMode::ReadWrite)?;
        let src = p.source_path()?;
        let n = p.meta()?.scope.len();
        p.close()?;
        (src, n)
    } else {
        create_project(worker_cfg.clone(), item)?
    };
    let p = Project::open(&item.project_path, OpenMode::ReadWrite)?;
    let mut config = sbwb_pipeline::scheduler::config_for(&p, &src, worker_cfg)?;
    config.lexicon = lexicon.clone();
    p.close()?;
    let s = Scheduler::start(config)?;
    let mut done = 0;
    let mut cancelled = false;
    for ev in s.events().iter() {
        if stop.load(Ordering::SeqCst) && !cancelled {
            s.cancel();
            cancelled = true;
        }
        match ev {
            PipelineEvent::Unit { ok: true, .. } => {
                done += 1;
                if done % 5 == 0 {
                    let slot = app.state::<QueueSlot>();
                    let guard = slot.items.lock();
                    if let Ok(mut items) = guard {
                        if let Some(it) = items.iter_mut().find(|i| i.id == item.id) {
                            it.done = done / 3; // three stages per page
                            it.total = total;
                        }
                    }
                    emit(app);
                }
            }
            PipelineEvent::Finished {
                done: d,
                cancelled: c,
                ..
            } => {
                s.join();
                return Ok((d, total, c));
            }
            _ => {}
        }
    }
    s.join();
    Ok((done / 3, total, cancelled))
}
