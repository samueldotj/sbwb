//! Application state shared by commands.

use std::path::PathBuf;
use std::sync::Mutex;

use sbwb_store::Project;
use sbwb_worker::{Worker, WorkerConfig};

use crate::paths::Resources;

pub struct OpenProject {
    pub store: Project,
    /// Extracted source PDF in the cache directory (renderer input, M2).
    #[allow(dead_code)]
    pub source_path: PathBuf,
}

pub struct AppState {
    pub resources: Resources,
    pub project: Mutex<Option<OpenProject>>,
    /// A worker kept warm for inspection and single renders.
    pub worker: Mutex<Option<Worker>>,
}

impl AppState {
    pub fn new(resources: Resources) -> Self {
        Self {
            resources,
            project: Mutex::new(None),
            worker: Mutex::new(None),
        }
    }

    pub fn worker_config(&self) -> WorkerConfig {
        WorkerConfig {
            pdfium_dir: Some(self.resources.pdfium_dir.clone()),
            tessdata_dir: Some(self.resources.tessdata_dir.clone()),
            ..Default::default()
        }
    }

    /// Run `f` against the shared worker, respawning it if it died.
    pub fn with_worker<T>(
        &self,
        f: impl FnOnce(&mut Worker) -> sbwb_core::Result<T>,
    ) -> sbwb_core::Result<T> {
        let mut guard = self
            .worker
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
