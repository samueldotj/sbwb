//! Pipeline control commands (PIPE-02, UX-02): start, pause, resume, cancel,
//! retry failed, status. Events are forwarded to the webview as
//! `pipeline:event`; page-state changes also raise `project:changed`.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use sbwb_core::PageStatus;
use sbwb_pipeline::{PipelineEvent, PipelineState, Scheduler, Status};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, CmdResult, CommandError};

pub const EVENT_PIPELINE: &str = "pipeline:event";

#[derive(Default)]
pub struct PipelineSlot {
    pub scheduler: Mutex<Option<Scheduler>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineStatus {
    pub active: bool,
    pub status: Option<Status>,
}

fn slot(app: &AppHandle) -> State<'_, PipelineSlot> {
    app.state::<PipelineSlot>()
}

/// Whether an interactive pipeline is running (the book queue must not
/// run at the same time, PRJ-06 resource limits).
pub fn is_active(app: &AppHandle) -> bool {
    slot(app)
        .scheduler
        .lock()
        .ok()
        .map(|g| g.as_ref().map(|s| !s.is_finished()).unwrap_or(false))
        .unwrap_or(false)
}

/// Start the plan for every queued page. Returns an error if one is running.
pub fn start_pipeline(app: &AppHandle) -> CmdResult<Status> {
    if crate::commands::queue::is_running(app) {
        return Err(CommandError::new(
            "conflict",
            "the book queue is processing; stop it before processing this book",
        ));
    }
    let state = app.state::<AppState>();
    let pslot = slot(app);
    let mut guard = pslot
        .scheduler
        .lock()
        .map_err(|_| CommandError::new("other", "pipeline mutex poisoned"))?;
    if let Some(s) = guard.as_ref() {
        if !s.is_finished() {
            return Err(CommandError::new(
                "conflict",
                "processing is already running",
            ));
        }
    }
    let config = {
        let pguard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = pguard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        if p.store.is_read_only() {
            return Err(CommandError::new("conflict", "the book is open read-only"));
        }
        let mut c =
            sbwb_pipeline::scheduler::config_for(&p.store, &p.source_path, state.worker_config())?;
        c.lexicon = sbwb_text::LexiconPaths::in_dir(&state.resources.lexicon_dir);
        c
    };
    let scheduler = Scheduler::start(config)?;
    let status = scheduler.status();
    let rx = scheduler.events().clone();
    *guard = Some(scheduler);

    // Forward events; coalesce project refreshes to at most every 300 ms.
    let handle = app.clone();
    std::thread::Builder::new()
        .name("pipeline-events".into())
        .spawn(move || {
            let mut last_refresh = Instant::now() - Duration::from_secs(1);
            let mut dirty = false;
            loop {
                match rx.recv_timeout(Duration::from_millis(300)) {
                    Ok(ev) => {
                        let is_unit = matches!(ev, PipelineEvent::Unit { .. });
                        let is_end = matches!(ev, PipelineEvent::Finished { .. });
                        let _ = handle.emit(EVENT_PIPELINE, &ev);
                        if is_unit || is_end {
                            dirty = true;
                        }
                        if dirty && (is_end || last_refresh.elapsed() > Duration::from_millis(300))
                        {
                            let _ =
                                handle.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
                            last_refresh = Instant::now();
                            dirty = false;
                        }
                        if is_end {
                            break;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if dirty {
                            let _ =
                                handle.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
                            last_refresh = Instant::now();
                            dirty = false;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .map_err(|e| CommandError::new("other", e))?;
    Ok(status)
}

#[tauri::command]
pub fn pipeline_start(app: AppHandle) -> CmdResult<Status> {
    start_pipeline(&app)
}

#[tauri::command]
pub fn pipeline_pause(app: AppHandle) -> CmdResult<()> {
    with_scheduler(&app, |s| s.pause())
}

#[tauri::command]
pub fn pipeline_resume(app: AppHandle) -> CmdResult<()> {
    with_scheduler(&app, |s| s.resume())
}

#[tauri::command]
pub fn pipeline_cancel(app: AppHandle) -> CmdResult<()> {
    with_scheduler(&app, |s| s.cancel())
}

fn with_scheduler(app: &AppHandle, f: impl FnOnce(&Scheduler)) -> CmdResult<()> {
    let pslot = slot(app);
    let guard = pslot
        .scheduler
        .lock()
        .map_err(|_| CommandError::new("other", "pipeline mutex poisoned"))?;
    match guard.as_ref() {
        Some(s) if !s.is_finished() => {
            f(s);
            Ok(())
        }
        _ => Err(CommandError::new("not_found", "nothing is running")),
    }
}

/// Re-queue failed pages and start again (PIPE-02 retry).
#[tauri::command]
pub fn pipeline_retry_failed(app: AppHandle) -> CmdResult<Status> {
    let state = app.state::<AppState>();
    {
        let pguard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = pguard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        let mut n = 0;
        for page in p.store.pages()? {
            if page.status == PageStatus::Failed {
                p.store
                    .set_page_status(page.index, PageStatus::Queued, None)?;
                n += 1;
            }
        }
        if n == 0 {
            return Err(CommandError::new("not_found", "no failed pages"));
        }
    }
    let _ = app.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
    start_pipeline(&app)
}

#[tauri::command]
pub fn pipeline_status(app: AppHandle) -> CmdResult<PipelineStatus> {
    let pslot = slot(&app);
    let guard = pslot
        .scheduler
        .lock()
        .map_err(|_| CommandError::new("other", "pipeline mutex poisoned"))?;
    Ok(match guard.as_ref() {
        Some(s) => {
            let st = s.status();
            PipelineStatus {
                active: !s.is_finished() && st.state != PipelineState::Done,
                status: Some(st),
            }
        }
        None => PipelineStatus {
            active: false,
            status: None,
        },
    })
}

/// Stop processing before the project closes (checkpoint at page boundary).
pub fn stop_for_close(app: &AppHandle) {
    if let Ok(mut guard) = slot(app).scheduler.lock() {
        if let Some(s) = guard.take() {
            if !s.is_finished() {
                s.cancel();
                s.join();
            }
        }
    }
}
