use sbwb_core::{PageIndex, Rect, RegionId, RunId, SpanId};
use serde::{Deserialize, Serialize};

/// A recognized word inside an OCR run (words are stored per run; the
/// index is their position in that run's word list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordRef {
    pub run: RunId,
    pub index: u32,
}

/// A source anchor: where a span's text came from on the scan (PROV-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub page: PageIndex,
    pub region: Option<RegionId>,
    pub words: Vec<WordRef>,
    pub bbox: Rect,
    /// True when only the enclosing line/region is known, not the word.
    pub approximate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanOrigin {
    /// Straight from OCR.
    Ocr,
    /// Produced by an automatic text-pass rule.
    AutoApplied,
    /// Accepted proposal.
    Accepted,
    /// Typed by a person.
    Manual,
    /// User text with no scan source.
    Inserted,
}

/// A unit of effective text with its anchors. One span per word keeps
/// anchors precise; joins merge two spans into one with two anchors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub id: SpanId,
    pub page: PageIndex,
    /// Position in the page's reading order.
    pub seq: u32,
    pub region: Option<RegionId>,
    pub text: String,
    pub anchors: Vec<Anchor>,
    pub origin: SpanOrigin,
    /// Native OCR confidence 0-100 for `Ocr` spans; `None` after any edit
    /// (edits never inherit confidence, PROV-01).
    pub confidence: Option<f32>,
    /// Trailing separator: " " between words, "\n" at a paragraph end,
    /// "" when the next span continues the same word (cross-page join).
    pub trailing: String,
    pub revision: u64,
    /// Protected words are never auto-corrected (TXT-01).
    pub protected: bool,
    /// Structural marker copied from the region: heading level etc.
    pub structure: sbwb_layout::WordStructure,
    /// First span of a paragraph.
    pub paragraph_start: bool,
}
