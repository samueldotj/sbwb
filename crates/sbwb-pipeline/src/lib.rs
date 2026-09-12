//! Processing pipeline (PIPE-01, PIPE-02, NFR-04, NFR-06, NFR-07).
//!
//! A [`Scheduler`] runs the local plan for one project: it dispatches page
//! units to a bounded pool of worker processes, records every run with its
//! model and settings, commits results page by page, and reports progress
//! through [`PipelineEvent`]s. Pause and cancel take effect at page
//! boundaries; a unit that exceeds its timeout is failed and its worker is
//! replaced.

pub mod events;
pub mod scheduler;
pub mod settings;

pub use events::{PipelineEvent, PipelineState, StageProgress};
pub use scheduler::{Scheduler, SchedulerConfig, Status};
pub use settings::ProcessingSettings;
