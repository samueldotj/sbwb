//! Export choices (EXP-01..06): what to export, page structure, furniture
//! policy, inclusion policies, formatting preset, and document metadata.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CopyKind {
    /// Current text as-is, unresolved words highlighted, one comment per flag.
    #[default]
    Working,
    /// No annotations; every page in scope must be approved.
    Clean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PageStructure {
    /// A Word page break (or section break) after each source page.
    #[default]
    Mirror,
    /// No breaks; furniture handled per the stated fallback.
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FurniturePolicy {
    /// Running head, page number, side note, footer note styles (D-12).
    #[default]
    StyledParagraphs,
    /// Native Word headers and footers, one next-page section per page.
    NativeHeadersFooters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TablePolicy {
    /// Emit the recognised text as paragraphs, with a warning.
    #[default]
    Text,
    /// Preserve the region as an image.
    Image,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inclusion {
    pub marginalia: bool,
    pub footnotes: bool,
    pub page_numbers: bool,
    pub catchwords: bool,
    pub illustrations: bool,
    pub uncertain: bool,
    pub tables: TablePolicy,
}

impl Default for Inclusion {
    fn default() -> Self {
        Self {
            marginalia: true,
            footnotes: true,
            page_numbers: true,
            catchwords: false,
            illustrations: true,
            uncertain: false,
            tables: TablePolicy::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaperSize {
    #[default]
    A4,
    Letter,
    /// Width and height in points.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropCap {
    pub enabled: bool,
    /// Lines the drop cap spans (2..5).
    pub lines: u8,
}

impl Default for DropCap {
    fn default() -> Self {
        Self {
            enabled: false,
            lines: 3,
        }
    }
}

/// A reusable formatting preset (EXP-06).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormatPreset {
    pub name: String,
    pub paper: PaperSize,
    pub custom_width_pt: f64,
    pub custom_height_pt: f64,
    pub margin_top_mm: f64,
    pub margin_bottom_mm: f64,
    pub margin_left_mm: f64,
    pub margin_right_mm: f64,
    pub body_font: String,
    pub body_size_pt: f64,
    /// Space after a body paragraph, in points.
    pub paragraph_spacing_pt: f64,
    /// Line spacing as a multiple of the font size.
    pub line_spacing: f64,
    pub heading_font: String,
    pub note_size_pt: f64,
    pub drop_cap: DropCap,
}

impl Default for FormatPreset {
    fn default() -> Self {
        Self {
            name: "Book".into(),
            paper: PaperSize::A4,
            custom_width_pt: 595.0,
            custom_height_pt: 842.0,
            margin_top_mm: 25.0,
            margin_bottom_mm: 25.0,
            margin_left_mm: 25.0,
            margin_right_mm: 25.0,
            body_font: "Georgia".into(),
            body_size_pt: 11.0,
            paragraph_spacing_pt: 6.0,
            line_spacing: 1.15,
            heading_font: "Georgia".into(),
            note_size_pt: 9.0,
            drop_cap: DropCap::default(),
        }
    }
}

impl FormatPreset {
    pub fn page_size_pt(&self) -> (f64, f64) {
        match self.paper {
            PaperSize::A4 => (595.276, 841.89),
            PaperSize::Letter => (612.0, 792.0),
            PaperSize::Custom => (
                self.custom_width_pt.max(200.0),
                self.custom_height_pt.max(200.0),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DocMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub copy: CopyKind,
    pub structure: PageStructure,
    pub furniture: FurniturePolicy,
    pub include: Inclusion,
    /// Working copy: unresolved issues scored strictly below this are
    /// highlighted and commented; unscored ones always are.
    pub flag_threshold: u8,
    pub archive: bool,
    pub preset: FormatPreset,
    pub metadata: DocMetadata,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            copy: CopyKind::Working,
            structure: PageStructure::Mirror,
            furniture: FurniturePolicy::StyledParagraphs,
            include: Inclusion::default(),
            flag_threshold: 90,
            archive: false,
            preset: FormatPreset::default(),
            metadata: DocMetadata::default(),
        }
    }
}

impl ExportSettings {
    /// One-line explanation of the furniture handling for the sheet
    /// (EXP-02 "must state how page-specific furniture is handled").
    pub fn furniture_explanation(&self) -> String {
        match (self.structure, self.furniture) {
            (PageStructure::Mirror, FurniturePolicy::StyledParagraphs) => "Running heads, page numbers, side notes, and footnotes become styled paragraphs in the text; a page break follows each source page.".into(),
            (PageStructure::Mirror, FurniturePolicy::NativeHeadersFooters) => "Running heads go into Word page headers, footers and side notes into Word page footers; every source page ends in a next-page section break.".into(),
            (PageStructure::Continuous, FurniturePolicy::StyledParagraphs) => "No page breaks. Running heads are omitted (recorded as excluded); page numbers, side notes, and footnotes stay as styled paragraphs where they occur.".into(),
            (PageStructure::Continuous, FurniturePolicy::NativeHeadersFooters) => "No page breaks, so native per-page headers are not possible: running heads are omitted (recorded as excluded) and the remaining furniture falls back to styled paragraphs.".into(),
        }
    }
}
