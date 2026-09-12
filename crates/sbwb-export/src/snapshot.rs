//! The immutable export snapshot (EXP-04): everything the writer and the
//! validator need, copied out of the project so later edits cannot change
//! an export in flight.

use sbwb_core::{PageIndex, Rect, RegionId, SpanId};
use sbwb_layout::{RegionKind, WordStructure};
use sbwb_text::SpanOrigin;
use serde::{Deserialize, Serialize};

use crate::settings::ExportSettings;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub source_name: String,
    pub source_pages: u32,
    pub source_blake3: String,
    /// Metadata as read from the PDF (EXP-06 "PDF metadata" area).
    pub pdf_title: Option<String>,
    pub pdf_author: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionSnap {
    pub id: RegionId,
    pub kind: RegionKind,
    pub order: u32,
    pub structure: WordStructure,
    pub bbox: Rect,
    pub anchor_y: Option<f64>,
}

/// An unresolved issue attached to a span (working-copy flags).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlagSnap {
    pub kind: String,
    pub score: Option<u8>,
    pub reason: String,
    pub deferred: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanSnap {
    pub id: SpanId,
    pub region: Option<RegionId>,
    pub text: String,
    pub trailing: String,
    pub origin: SpanOrigin,
    pub confidence: Option<f32>,
    pub paragraph_start: bool,
    pub structure: WordStructure,
    /// Anchor boxes on the scan; pages other than the span's own mean a
    /// cross-page join (EXP-02).
    pub anchors: Vec<(u32, Rect)>,
    pub flag: Option<FlagSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageSnap {
    pub index: u32,
    pub label: Option<String>,
    pub width_pt: f64,
    pub height_pt: f64,
    /// `none`, `current`, `outdated`.
    pub approval: String,
    pub approved_outstanding: u32,
    pub unresolved: u32,
    pub deferred: u32,
    pub text_done: bool,
    pub regions: Vec<RegionSnap>,
    pub spans: Vec<SpanSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSnapshot {
    pub id: String,
    pub created_at: String,
    pub app_version: String,
    pub book: BookInfo,
    pub settings: ExportSettings,
    pub pages: Vec<PageSnap>,
}

impl ExportSnapshot {
    pub fn page(&self, index: u32) -> Option<&PageSnap> {
        self.pages.iter().find(|p| p.index == index)
    }

    /// Pages in scope that are not currently approved (clean-copy gate).
    pub fn pages_not_approved(&self) -> Vec<u32> {
        self.pages
            .iter()
            .filter(|p| p.approval != "current")
            .map(|p| p.index)
            .collect()
    }

    pub fn word_count(&self) -> usize {
        self.pages.iter().map(|p| p.spans.len()).sum()
    }
}

impl PageSnap {
    pub fn page_index(&self) -> PageIndex {
        PageIndex(self.index)
    }
}
