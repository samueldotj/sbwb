//! Conservative page preparation. Every transform is recorded in the
//! [`PrepReport`] so evidence stays inspectable (IMG-01).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepSettings {
    pub deskew: bool,
    /// Maximum absolute skew angle (degrees) that will be corrected.
    pub max_skew_deg: f64,
    pub despeckle: bool,
}

impl Default for PrepSettings {
    fn default() -> Self {
        Self {
            deskew: true,
            max_skew_deg: 5.0,
            despeckle: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrepReport {
    pub input_px: (u32, u32),
    pub output_px: (u32, u32),
    pub skew_deg: f64,
    pub deskewed: bool,
    pub despeckled: bool,
    pub warnings: Vec<String>,
}

/// Prepare a page bitmap for recognition. M0 stub: passes the image through
/// and reports its geometry; deskew arrives in M3.4.
pub fn prepare(img: &image::RgbImage, settings: &PrepSettings) -> (image::RgbImage, PrepReport) {
    let _ = settings;
    let report = PrepReport {
        input_px: (img.width(), img.height()),
        output_px: (img.width(), img.height()),
        ..Default::default()
    };
    (img.clone(), report)
}
