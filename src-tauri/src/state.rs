//! Application state shared by commands.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sbwb_store::Project;
use sbwb_worker::{Worker, WorkerConfig};

use crate::paths::Resources;
use crate::render::RenderCache;

pub struct OpenProject {
    pub store: Project,
    /// Extracted source PDF in the cache directory (renderer input).
    pub source_path: PathBuf,
    /// Preview render cache for this project (D-14).
    pub render_cache: Arc<RenderCache>,
}

pub struct AppState {
    pub resources: Resources,
    pub project: Mutex<Option<OpenProject>>,
    /// A worker kept warm for inspection and OCR probes.
    pub worker: Mutex<Option<Worker>>,
    /// A separate worker for preview renders so a long OCR call never
    /// blocks navigation (NFR-03).
    pub render_worker: Mutex<Option<Worker>>,
    /// Renders for the page on screen that are waiting for or using the
    /// render worker; prefetch gives way to them.
    render_waiting: AtomicUsize,
}

impl AppState {
    pub fn new(resources: Resources) -> Self {
        Self {
            resources,
            project: Mutex::new(None),
            worker: Mutex::new(None),
            render_worker: Mutex::new(None),
            render_waiting: AtomicUsize::new(0),
        }
    }

    pub fn worker_config(&self) -> WorkerConfig {
        WorkerConfig {
            pdfium_dir: Some(self.resources.pdfium_dir.clone()),
            tessdata_dir: Some(self.resources.tessdata_dir.clone()),
            ..Default::default()
        }
    }

    fn with_slot<T>(
        &self,
        slot: &Mutex<Option<Worker>>,
        f: impl FnOnce(&mut Worker) -> sbwb_core::Result<T>,
    ) -> sbwb_core::Result<T> {
        let mut guard = slot
            .lock()
            .map_err(|_| sbwb_core::SbwbError::other("worker mutex poisoned"))?;
        if guard.as_mut().map(|w| !w.is_alive()).unwrap_or(true) {
            *guard = Some(Worker::spawn(self.worker_config())?);
        }
        let w = guard.as_mut().expect("worker present");
        let r = f(w);
        if let Err(sbwb_core::SbwbError::Timeout(_)) = &r {
            *guard = None;
        }
        r
    }

    /// Run `f` against the shared worker, respawning it if it died.
    pub fn with_worker<T>(
        &self,
        f: impl FnOnce(&mut Worker) -> sbwb_core::Result<T>,
    ) -> sbwb_core::Result<T> {
        self.with_slot(&self.worker, f)
    }

    /// Render for the page on screen.
    pub fn with_render_worker<T>(
        &self,
        f: impl FnOnce(&mut Worker) -> sbwb_core::Result<T>,
    ) -> sbwb_core::Result<T> {
        self.render_waiting.fetch_add(1, Ordering::SeqCst);
        let r = self.with_slot(&self.render_worker, f);
        self.render_waiting.fetch_sub(1, Ordering::SeqCst);
        r
    }

    /// Speculative render. Returns `None` without doing the work when a
    /// render for the page on screen is waiting, so navigation is held up by
    /// at most the one prefetch already running (NFR-03).
    pub fn with_render_worker_idle<T>(
        &self,
        f: impl FnOnce(&mut Worker) -> sbwb_core::Result<T>,
    ) -> sbwb_core::Result<Option<T>> {
        if self.render_waiting.load(Ordering::SeqCst) > 0 {
            return Ok(None);
        }
        self.with_slot(&self.render_worker, |w| {
            if self.render_waiting.load(Ordering::SeqCst) > 0 {
                return Ok(None);
            }
            f(w).map(Some)
        })
    }
}

/// Error type crossing the IPC boundary: a code the UI can branch on plus a
/// human message (design 4.2 needs "needs password", "locked", etc.).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<sbwb_core::SbwbError> for CommandError {
    fn from(e: sbwb_core::SbwbError) -> Self {
        use sbwb_core::SbwbError as E;
        let code = match &e {
            E::InvalidInput(_) => "invalid_input",
            E::Integrity(_) => "integrity",
            E::NotFound(_) => "not_found",
            E::Conflict(_) => "conflict",
            E::Cancelled => "cancelled",
            E::Timeout(_) => "timeout",
            E::ResourceLimit(_) => "resource_limit",
            E::Io(_) => "io",
            E::Serde(_) => "serde",
            E::Other(_) => "other",
        };
        tracing::warn!(code, "command failed: {e}");
        Self {
            code: code.into(),
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        CommandError::new("serde", e)
    }
}

impl CommandError {
    pub fn new(code: &str, message: impl std::fmt::Display) -> Self {
        tracing::warn!(code, "command failed: {message}");
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}

pub type CmdResult<T> = std::result::Result<T, CommandError>;
