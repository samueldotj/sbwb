//! Background preview generation for the Pages view (IMG-01, NFR-03).
//!
//! The page grid only asks for thumbnails near the visible area, so on a
//! large book most pages wait until they are scrolled to. This job renders
//! every listed page into the preview cache ahead of time, on all preview
//! render workers at once, and gives way to renders for the page on screen.
//! It can be stopped and started again; a new start replaces the old run.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sbwb_core::PageIndex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, CmdResult, CommandError};

pub const EVENT_PREVIEWS: &str = "previews:progress";

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct PreviewProgress {
    /// Which run this is about; the UI ignores the others.
    pub run: u32,
    pub done: u32,
    pub total: u32,
    pub running: bool,
}

/// One page to render and the scale the grid will ask for.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PreviewItem {
    pub page: u32,
    pub scale: f64,
}

static NEXT_RUN: AtomicU32 = AtomicU32::new(1);

struct Run {
    id: u32,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicU32>,
    total: u32,
}

#[derive(Default)]
pub struct PreviewSlot {
    run: Mutex<Option<Run>>,
}

impl PreviewSlot {
    fn stop_current(&self) {
        if let Ok(mut r) = self.run.lock() {
            if let Some(run) = r.take() {
                run.cancel.store(true, Ordering::SeqCst);
            }
        }
    }
}

/// Render these pages into the preview cache in the background.
#[tauri::command]
pub fn previews_start(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, PreviewSlot>,
    items: Vec<PreviewItem>,
) -> CmdResult<PreviewProgress> {
    slot.stop_current();
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
    let total = items.len() as u32;
    let todo: VecDeque<PreviewItem> = items
        .into_iter()
        .filter(|i| cache.cached(PageIndex(i.page), i.scale).is_none())
        .collect();
    let done = Arc::new(AtomicU32::new(total - todo.len() as u32));
    let cancel = Arc::new(AtomicBool::new(false));
    let run = NEXT_RUN.fetch_add(1, Ordering::SeqCst);
    let progress = PreviewProgress {
        run,
        done: done.load(Ordering::SeqCst),
        total,
        running: !todo.is_empty(),
    };
    if todo.is_empty() {
        return Ok(progress);
    }
    if let Ok(mut r) = slot.run.lock() {
        *r = Some(Run {
            id: run,
            cancel: cancel.clone(),
            done: done.clone(),
            total,
        });
    }
    let queue = Arc::new(Mutex::new(todo));
    let threads = state.render_pool_size();
    let last_emit = Arc::new(Mutex::new(Instant::now()));
    let left = Arc::new(AtomicU32::new(threads as u32));
    for _ in 0..threads {
        let (app, queue, cancel, done, source, cache, last_emit, left) = (
            app.clone(),
            queue.clone(),
            cancel.clone(),
            done.clone(),
            source.clone(),
            cache.clone(),
            last_emit.clone(),
            left.clone(),
        );
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            loop {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let Some(item) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
                    break;
                };
                let page = PageIndex(item.page);
                // the grid may have rendered it meanwhile
                let rendered = if cache.cached(page, item.scale).is_some() {
                    Ok(Some(()))
                } else {
                    state
                        .with_render_worker_idle(|w| cache.ensure(w, &source, page, item.scale))
                        .map(|r| r.map(|_| ()))
                };
                match rendered {
                    Ok(Some(())) => {
                        done.fetch_add(1, Ordering::SeqCst);
                    }
                    Ok(None) => {
                        // A render for the page on screen goes first. Move on:
                        // the grid often renders this page itself meanwhile.
                        if let Ok(mut q) = queue.lock() {
                            q.push_back(item);
                        }
                        std::thread::sleep(Duration::from_millis(15));
                        continue;
                    }
                    Err(e) => {
                        tracing::debug!("preview {} @ {}: {e}", item.page, item.scale);
                        done.fetch_add(1, Ordering::SeqCst);
                    }
                }
                let due = last_emit
                    .lock()
                    .map(|mut t| {
                        let due = t.elapsed() >= Duration::from_millis(250);
                        if due {
                            *t = Instant::now();
                        }
                        due
                    })
                    .unwrap_or(false);
                if due {
                    let _ = app.emit(
                        EVENT_PREVIEWS,
                        PreviewProgress {
                            run,
                            done: done.load(Ordering::SeqCst),
                            total,
                            running: true,
                        },
                    );
                }
            }
            // the last thread out reports the end of the run
            if left.fetch_sub(1, Ordering::SeqCst) == 1 {
                let _ = app.emit(
                    EVENT_PREVIEWS,
                    PreviewProgress {
                        run,
                        done: done.load(Ordering::SeqCst),
                        total,
                        running: false,
                    },
                );
                let slot = app.state::<PreviewSlot>();
                if let Ok(mut r) = slot.run.lock() {
                    if r.as_ref().is_some_and(|current| current.id == run) {
                        *r = None;
                    }
                };
            }
        });
    }
    Ok(progress)
}

/// Stop generating previews; what is already rendered stays cached.
#[tauri::command]
pub fn previews_stop(app: AppHandle, slot: State<'_, PreviewSlot>) -> CmdResult<()> {
    let snapshot = slot.run.lock().ok().and_then(|r| {
        r.as_ref()
            .map(|run| (run.id, run.done.load(Ordering::SeqCst), run.total))
    });
    slot.stop_current();
    if let Some((run, done, total)) = snapshot {
        let _ = app.emit(
            EVENT_PREVIEWS,
            PreviewProgress {
                run,
                done,
                total,
                running: false,
            },
        );
    }
    Ok(())
}
