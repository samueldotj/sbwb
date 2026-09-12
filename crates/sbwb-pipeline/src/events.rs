use sbwb_core::{PageIndex, Stage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    Idle,
    Running,
    Paused,
    /// Cancel requested; running units are finishing.
    Stopping,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageProgress {
    pub stage: Stage,
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    pub running: u32,
    pub elapsed_ms: u64,
    /// Present only once enough units have finished to make it meaningful.
    pub eta_ms: Option<u64>,
    /// Mean seconds per unit so far.
    pub secs_per_unit: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipelineEvent {
    State {
        state: PipelineState,
    },
    Progress {
        stages: Vec<StageProgress>,
        /// Human activity line, e.g. "ocr p.14".
        activity: String,
    },
    Unit {
        stage: Stage,
        page: PageIndex,
        ok: bool,
        elapsed_ms: u64,
        /// Words and mean confidence for OCR units.
        words: Option<u32>,
        mean_confidence: Option<f32>,
        error: Option<String>,
        warnings: Vec<String>,
    },
    Log {
        ts: String,
        level: String,
        text: String,
    },
    Finished {
        done: u32,
        failed: u32,
        cancelled: bool,
    },
}
