use sbwb_core::{Rect, RegionId};
use serde::{Deserialize, Serialize};

/// Region classes (LAY-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Body,
    Heading,
    Header,
    Footer,
    Marginalia,
    Footnote,
    PageNumber,
    Catchword,
    Illustration,
    Table,
    Uncertain,
    /// Explicit user exclusion.
    Ignore,
}

impl RegionKind {
    pub const ALL: [RegionKind; 12] = [
        RegionKind::Body,
        RegionKind::Heading,
        RegionKind::Header,
        RegionKind::Footer,
        RegionKind::Marginalia,
        RegionKind::Footnote,
        RegionKind::PageNumber,
        RegionKind::Catchword,
        RegionKind::Illustration,
        RegionKind::Table,
        RegionKind::Uncertain,
        RegionKind::Ignore,
    ];
    pub fn label(self) -> &'static str {
        match self {
            RegionKind::Body => "body",
            RegionKind::Heading => "heading",
            RegionKind::Header => "header",
            RegionKind::Footer => "footer",
            RegionKind::Marginalia => "marginalia",
            RegionKind::Footnote => "footnote",
            RegionKind::PageNumber => "page no.",
            RegionKind::Catchword => "catchword",
            RegionKind::Illustration => "illustration",
            RegionKind::Table => "table",
            RegionKind::Uncertain => "uncertain",
            RegionKind::Ignore => "ignore",
        }
    }
    pub fn is_text(self) -> bool {
        !matches!(self, RegionKind::Illustration | RegionKind::Ignore)
    }
    /// Default export placement label shown in the reading-order list
    /// (design 4.4); the export policy itself is decided in M7.
    pub fn placement(self) -> &'static str {
        match self {
            RegionKind::Header => "→ running head",
            RegionKind::Footer => "→ footer",
            RegionKind::Marginalia => "→ side note",
            RegionKind::Footnote => "→ footnote",
            RegionKind::PageNumber => "→ page number",
            RegionKind::Catchword => "archive only",
            RegionKind::Illustration => "→ image",
            RegionKind::Ignore => "excluded",
            RegionKind::Uncertain => "review",
            _ => "→ body",
        }
    }
    /// Reading-order rank between classes (lower first).
    pub fn rank(self) -> u8 {
        match self {
            RegionKind::Header => 0,
            RegionKind::PageNumber => 1,
            RegionKind::Heading
            | RegionKind::Body
            | RegionKind::Table
            | RegionKind::Illustration => 2,
            RegionKind::Marginalia => 3,
            RegionKind::Footnote => 4,
            RegionKind::Footer => 5,
            RegionKind::Catchword => 6,
            RegionKind::Uncertain => 7,
            RegionKind::Ignore => 8,
        }
    }
}

/// Word structure assigned to a region (LAY-02 "In Word").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WordStructure {
    #[default]
    Text,
    Heading1,
    Heading2,
    Heading3,
    TableCell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: RegionId,
    pub kind: RegionKind,
    pub bbox: Rect,
    /// Position in reading order, 0-based.
    pub order: u32,
    pub structure: WordStructure,
    /// Column index for body text, when columns were detected.
    pub column: Option<u8>,
    /// Detection confidence 0..1 (heuristic score, not a probability).
    pub score: f32,
    /// Whether a person created or edited this region.
    pub manual: bool,
    /// Number of detected text lines inside the region.
    #[serde(default)]
    pub line_count: u32,
    /// For side notes: the page-space y of the body line they sit beside.
    #[serde(default)]
    pub anchor_y: Option<f64>,
}

impl Region {
    pub fn new(kind: RegionKind, bbox: Rect, order: u32) -> Self {
        Self {
            id: RegionId::new(),
            kind,
            bbox,
            order,
            structure: WordStructure::Text,
            column: None,
            score: 1.0,
            manual: false,
            line_count: 0,
            anchor_y: None,
        }
    }
}
