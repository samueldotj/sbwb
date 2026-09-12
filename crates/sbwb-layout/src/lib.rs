//! Layout analysis (LAY-01, LAY-03, D-11).
//!
//! [`analyze`] works from the page bitmap itself, independently of OCR:
//! connected components are smeared into line blobs, line blobs into
//! blocks, blocks are classified by position and text size, and a reading
//! order is derived from structure. OCR words are used afterwards for
//! coverage accounting (words outside regions, ink blobs with no words) and
//! Tesseract's own segmentation is compared as a second opinion.

pub mod analyze;
pub mod region;

pub use analyze::{analyze, AnalysisSettings, CoverageReport, LayoutInput, LayoutOutput};
pub use region::{Region, RegionKind, WordStructure};

/// Render resolution used for image analysis (fast; text is still ~14 px).
pub const ANALYSIS_DPI: u32 = 100;
