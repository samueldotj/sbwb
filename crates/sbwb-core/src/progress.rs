use serde::{Deserialize, Serialize};

/// Pipeline stages shown in the rail (design section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Import,
    Ocr,
    Layout,
    TextPass,
    AiProofread,
    Export,
}

impl Stage {
    pub const ALL: [Stage; 6] = [
        Stage::Import,
        Stage::Ocr,
        Stage::Layout,
        Stage::TextPass,
        Stage::AiProofread,
        Stage::Export,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Stage::Import => "Import",
            Stage::Ocr => "OCR",
            Stage::Layout => "Layout",
            Stage::TextPass => "Text pass",
            Stage::AiProofread => "AI proofread",
            Stage::Export => "Export",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Off,
    Queued,
    Running,
    Paused,
    Done,
    Failed,
}

/// A progress report for one stage (PIPE-02). `total` is `None` when the
/// amount of work is unknown; the UI then shows activity, not a percentage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub stage: Stage,
    pub state: StageState,
    pub done: u32,
    pub total: Option<u32>,
    pub failed: u32,
    pub elapsed_ms: u64,
    /// Estimated remaining milliseconds; only present when reliable.
    pub eta_ms: Option<u64>,
    /// Free-text activity, e.g. "ocr p.14".
    pub activity: String,
}

impl Progress {
    pub fn fraction(&self) -> Option<f64> {
        self.total
            .filter(|t| *t > 0)
            .map(|t| self.done as f64 / t as f64)
    }
}
