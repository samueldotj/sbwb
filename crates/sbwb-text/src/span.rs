use sbwb_core::{PageIndex, Rect, RegionId, SpanId, WordId};
use serde::{Deserialize, Serialize};

/// A source anchor: where a span's text came from on the scan (PROV-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub page: PageIndex,
    pub region: Option<RegionId>,
    pub word: Option<WordId>,
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

/// A unit of effective text with its anchors. The text-pass and review
/// layers operate on spans; export walks them in reading order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub id: SpanId,
    pub text: String,
    pub anchors: Vec<Anchor>,
    pub origin: SpanOrigin,
    /// Native OCR confidence 0-100 for `Ocr` spans; `None` after any edit
    /// (edits never inherit confidence, PROV-01).
    pub confidence: Option<f32>,
    /// Trailing whitespace/paragraph marker: " " normally, "\n" at a
    /// paragraph end, "" when joined with the next span (hyphen join).
    pub trailing: String,
    pub revision: u64,
}
