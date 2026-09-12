//! Per-book processing settings (UX-06, D-22, D-24), stored in the project
//! settings JSON under `"processing"`.

use sbwb_core::Result;
use sbwb_image::PrepSettings;
use sbwb_ocr::ModelPack;
use sbwb_store::Project;
use serde::{Deserialize, Serialize};

pub const KEY: &str = "processing";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProcessingSettings {
    pub model: ModelPack,
    /// Recognition render resolution (IMG-01: nominal 300).
    pub dpi: u32,
    pub deskew: bool,
    pub despeckle: bool,
    /// Text-pass auto-apply threshold, 0-100 (TXT-03, default 90).
    pub auto_apply_threshold: u8,
    /// OCR → layout → text pass after import without confirmation.
    pub run_stages_automatically: bool,
    /// Requested worker processes; capped by hardware (NFR-04, NFR-07).
    pub workers: u32,
    /// Lexicon profile for the text pass (TXT-01, P2.4).
    #[serde(default)]
    pub language: sbwb_text::LanguageProfile,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            model: ModelPack::EngBest,
            dpi: 300,
            deskew: true,
            despeckle: false,
            auto_apply_threshold: 90,
            run_stages_automatically: true,
            workers: default_workers(),
            language: sbwb_text::LanguageProfile::Modern,
        }
    }
}

/// Default worker count: half the logical cores, at least 1, at most 6.
pub fn default_workers() -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    ((cores / 2).max(1) as u32).min(6)
}

impl ProcessingSettings {
    pub fn prep(&self) -> PrepSettings {
        PrepSettings {
            deskew: self.deskew,
            despeckle: self.despeckle,
            ..Default::default()
        }
    }

    pub fn ocr(&self) -> sbwb_ocr::OcrSettings {
        sbwb_ocr::OcrSettings {
            model: self.model,
            segmentation: sbwb_ocr::Segmentation::Auto,
            dpi: self.dpi,
        }
    }

    /// Settings that change OCR output; used to decide whether a done page
    /// must be re-run after a settings change (PIPE-01).
    pub fn auto_apply_policy(&self) -> sbwb_text::AutoApplyPolicy {
        sbwb_text::AutoApplyPolicy {
            threshold: self.auto_apply_threshold,
            ..Default::default()
        }
    }

    pub fn ocr_fingerprint(&self) -> String {
        format!(
            "{:?}|{}|deskew={}|despeckle={}",
            self.model, self.dpi, self.deskew, self.despeckle
        )
    }

    pub fn load(project: &Project) -> Result<Self> {
        let all = project.settings()?;
        Ok(all
            .get(KEY)
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    pub fn save(&self, project: &Project) -> Result<()> {
        let mut all = project.settings()?;
        if !all.is_object() {
            all = serde_json::json!({});
        }
        all[KEY] = serde_json::to_value(self)?;
        project.set_settings(&all)
    }
}

/// Worker count actually used: the request, capped by cores and by memory
/// (about 1.5 GB per Tesseract worker keeps the 6 GB aggregate bound, NFR-04).
pub fn effective_workers(requested: u32) -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let mem_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let by_mem = ((mem_gb / 1.5).floor() as u32).max(1);
    requested.clamp(1, cores.max(1)).min(by_mem)
}
