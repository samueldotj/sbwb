//! Composition plan (EXP-02, EXP-03): the snapshot becomes an ordered list
//! of paragraphs per page with a placement and a source mapping. The
//! writer emits the plan; the validator checks the file against it.

use sbwb_core::{Rect, RegionId, SpanId};
use sbwb_layout::{RegionKind, WordStructure};
use serde::{Deserialize, Serialize};

use crate::settings::{CopyKind, ExportSettings, FurniturePolicy, PageStructure, TablePolicy};
use crate::snapshot::{ExportSnapshot, FlagSnap, PageSnap, RegionSnap, SpanSnap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    Body,
    Heading,
    RunningHead,
    PrintedPageNumber,
    SideNote,
    FooterNote,
    FootnoteText,
    TableText,
    Image,
    NativeHeader,
    NativeFooter,
}

impl Placement {
    pub fn style_id(self) -> &'static str {
        match self {
            Placement::Body => "SbwbBody",
            Placement::Heading => "Heading1",
            Placement::RunningHead => "RunningHead",
            Placement::PrintedPageNumber => "PrintedPageNumber",
            Placement::SideNote => "SideNote",
            Placement::FooterNote => "FooterNote",
            Placement::FootnoteText => "FootnoteText",
            Placement::TableText => "TableText",
            Placement::Image => "SbwbBody",
            Placement::NativeHeader => "RunningHead",
            Placement::NativeFooter => "FooterNote",
        }
    }
    /// Label shown in the reading-order list and the report.
    pub fn label(self) -> &'static str {
        match self {
            Placement::Body => "→ body",
            Placement::Heading => "→ heading",
            Placement::RunningHead => "→ running head",
            Placement::PrintedPageNumber => "→ page number",
            Placement::SideNote => "→ side note",
            Placement::FooterNote => "→ footer note",
            Placement::FootnoteText => "→ footnote",
            Placement::TableText => "→ table text",
            Placement::Image => "→ image",
            Placement::NativeHeader => "→ Word header",
            Placement::NativeFooter => "→ Word footer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedRun {
    pub text: String,
    pub span: Option<SpanId>,
    pub flag: Option<FlagSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedParagraph {
    pub page: u32,
    pub region: Option<RegionId>,
    pub placement: Placement,
    pub structure: WordStructure,
    pub runs: Vec<PlannedRun>,
    /// For images: the region box on the page in points.
    pub image: Option<Rect>,
}

impl PlannedParagraph {
    pub fn text(&self) -> String {
        self.runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>()
    }
    pub fn spans(&self) -> Vec<SpanId> {
        self.runs.iter().filter_map(|r| r.span).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageBreak {
    None,
    Page,
    Section,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedPage {
    pub index: u32,
    pub label: Option<String>,
    pub body: Vec<PlannedParagraph>,
    pub header: Vec<PlannedParagraph>,
    pub footer: Vec<PlannedParagraph>,
    pub break_after: PageBreak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exclusion {
    pub page: u32,
    pub region: Option<RegionId>,
    pub kind: String,
    pub words: u32,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanStats {
    pub pages: u32,
    pub paragraphs: u32,
    pub words: u32,
    pub running_heads: u32,
    pub page_numbers: u32,
    pub side_notes: u32,
    pub footer_notes: u32,
    pub footnotes: u32,
    pub headings: u32,
    pub images: u32,
    pub tables: u32,
    pub flags: u32,
    pub cross_page_words: u32,
    pub unanchored_words: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub pages: Vec<PlannedPage>,
    pub exclusions: Vec<Exclusion>,
    pub warnings: Vec<String>,
    pub stats: PlanStats,
    /// Words joined across a page boundary; each must appear exactly once.
    pub cross_page_words: Vec<String>,
}

fn paragraphs_of(
    page: &PageSnap,
    region: &RegionSnap,
    placement: Placement,
    settings: &ExportSettings,
    stats: &mut PlanStats,
) -> Vec<PlannedParagraph> {
    let mut out: Vec<PlannedParagraph> = Vec::new();
    // Native header/footer parts carry no review annotations.
    let flagged = settings.copy == CopyKind::Working
        && !matches!(placement, Placement::NativeHeader | Placement::NativeFooter);
    for s in page.spans.iter().filter(|s| s.region == Some(region.id)) {
        let structure = s.structure;
        let start = s.paragraph_start
            || out.is_empty()
            || out.last().map(|p| p.structure != structure).unwrap_or(true);
        if start {
            let placement = match (placement, structure) {
                (
                    Placement::Body,
                    WordStructure::Heading1 | WordStructure::Heading2 | WordStructure::Heading3,
                ) => Placement::Heading,
                (p, _) => p,
            };
            out.push(PlannedParagraph {
                page: page.index,
                region: Some(region.id),
                placement,
                structure,
                runs: vec![],
                image: None,
            });
        }
        let flag = if flagged {
            s.flag
                .as_ref()
                .filter(|f| {
                    f.score
                        .map(|sc| sc < settings.flag_threshold)
                        .unwrap_or(true)
                })
                .cloned()
        } else {
            None
        };
        if flag.is_some() {
            stats.flags += 1;
        }
        if s.anchors.iter().any(|(p, _)| *p != page.index) {
            stats.cross_page_words += 1;
        }
        if s.anchors.is_empty() {
            stats.unanchored_words += 1;
        }
        stats.words += 1;
        let para = out.last_mut().unwrap();
        // A paragraph's last trailing newline is implied by the paragraph.
        let text = format!(
            "{}{}",
            s.text,
            if s.trailing == "\n" {
                ""
            } else {
                s.trailing.as_str()
            }
        );
        para.runs.push(PlannedRun {
            text,
            span: Some(s.id),
            flag,
        });
    }
    // Trim trailing spaces at paragraph ends.
    for p in &mut out {
        if let Some(last) = p.runs.last_mut() {
            let trimmed = last.text.trim_end().to_string();
            last.text = trimmed;
        }
    }
    out
}

fn region_words(page: &PageSnap, region: &RegionSnap) -> u32 {
    page.spans
        .iter()
        .filter(|s| s.region == Some(region.id))
        .count() as u32
}

/// Insert side notes before the body paragraph they annotate: the first
/// body paragraph whose first anchor lies at or below the note's anchor.
fn insert_side_notes(
    body: &mut Vec<PlannedParagraph>,
    notes: Vec<(f64, Vec<PlannedParagraph>)>,
    page: &PageSnap,
) {
    for (y, paras) in notes {
        let at = body
            .iter()
            .position(|p| {
                if !matches!(
                    p.placement,
                    Placement::Body | Placement::Heading | Placement::TableText
                ) {
                    return false;
                }
                let first = p
                    .runs
                    .iter()
                    .find_map(|r| r.span)
                    .and_then(|id| page.spans.iter().find(|s| s.id == id));
                match first.and_then(|s| s.anchors.first()) {
                    Some((_, b)) => b.y + b.h >= y,
                    None => false,
                }
            })
            .unwrap_or(body.len());
        let mut tail = body.split_off(at);
        body.extend(paras);
        body.append(&mut tail);
    }
}

pub fn compose(snapshot: &ExportSnapshot) -> Plan {
    let settings = &snapshot.settings;
    let mut exclusions = Vec::new();
    let mut warnings = Vec::new();
    let mut stats = PlanStats::default();
    let mut cross_page_words = Vec::new();
    let native = settings.furniture == FurniturePolicy::NativeHeadersFooters
        && settings.structure == PageStructure::Mirror;
    if settings.furniture == FurniturePolicy::NativeHeadersFooters
        && settings.structure == PageStructure::Continuous
    {
        warnings.push("Continuous text cannot carry per-page native headers; furniture fell back to styled paragraphs and running heads were excluded.".into());
    }
    let mut pages = Vec::new();
    let n = snapshot.pages.len();
    for (pi, page) in snapshot.pages.iter().enumerate() {
        stats.pages += 1;
        let mut regions: Vec<&RegionSnap> = page.regions.iter().collect();
        regions.sort_by_key(|r| r.order);
        let mut head: Vec<PlannedParagraph> = Vec::new();
        let mut body: Vec<PlannedParagraph> = Vec::new();
        let mut notes: Vec<(f64, Vec<PlannedParagraph>)> = Vec::new();
        let mut footnotes: Vec<PlannedParagraph> = Vec::new();
        let mut footers: Vec<PlannedParagraph> = Vec::new();
        for r in regions {
            let words = region_words(page, r);
            let mut exclude = |why: &str| {
                exclusions.push(Exclusion {
                    page: page.index,
                    region: Some(r.id),
                    kind: r.kind.label().to_string(),
                    words,
                    why: why.to_string(),
                });
            };
            match r.kind {
                RegionKind::Ignore => exclude("marked ignore in Layout"),
                RegionKind::Catchword => {
                    if settings.include.catchwords {
                        body.extend(paragraphs_of(
                            page,
                            r,
                            Placement::FooterNote,
                            settings,
                            &mut stats,
                        ));
                    } else {
                        exclude("catchwords are archive-only");
                    }
                }
                RegionKind::Uncertain => {
                    if settings.include.uncertain {
                        body.extend(paragraphs_of(
                            page,
                            r,
                            Placement::Body,
                            settings,
                            &mut stats,
                        ));
                        warnings.push(format!(
                            "page {}: an uncertain region was exported as body text",
                            page.index + 1
                        ));
                    } else {
                        exclude("uncertain region not included");
                    }
                }
                RegionKind::Header => {
                    if settings.structure == PageStructure::Continuous {
                        exclude("running heads are omitted in continuous text");
                    } else {
                        let ps = paragraphs_of(
                            page,
                            r,
                            if native {
                                Placement::NativeHeader
                            } else {
                                Placement::RunningHead
                            },
                            settings,
                            &mut stats,
                        );
                        stats.running_heads += ps.len() as u32;
                        head.extend(ps);
                    }
                }
                RegionKind::PageNumber => {
                    if settings.include.page_numbers {
                        let ps = paragraphs_of(
                            page,
                            r,
                            if native {
                                Placement::NativeHeader
                            } else {
                                Placement::PrintedPageNumber
                            },
                            settings,
                            &mut stats,
                        );
                        stats.page_numbers += ps.len() as u32;
                        head.extend(ps);
                    } else {
                        exclude("source page numbers not included");
                    }
                }
                RegionKind::Marginalia => {
                    if settings.include.marginalia {
                        let ps = paragraphs_of(
                            page,
                            r,
                            if native {
                                Placement::NativeFooter
                            } else {
                                Placement::SideNote
                            },
                            settings,
                            &mut stats,
                        );
                        stats.side_notes += ps.len() as u32;
                        if native {
                            footers.extend(ps);
                        } else {
                            notes.push((r.anchor_y.unwrap_or(r.bbox.y), ps));
                        }
                    } else {
                        exclude("marginal notes not included");
                    }
                }
                RegionKind::Footnote => {
                    if settings.include.footnotes {
                        let ps =
                            paragraphs_of(page, r, Placement::FootnoteText, settings, &mut stats);
                        stats.footnotes += ps.len() as u32;
                        footnotes.extend(ps);
                    } else {
                        exclude("footnotes not included");
                    }
                }
                RegionKind::Footer => {
                    let ps = paragraphs_of(
                        page,
                        r,
                        if native {
                            Placement::NativeFooter
                        } else {
                            Placement::FooterNote
                        },
                        settings,
                        &mut stats,
                    );
                    stats.footer_notes += ps.len() as u32;
                    footers.extend(ps);
                }
                RegionKind::Illustration => {
                    if settings.include.illustrations {
                        stats.images += 1;
                        body.push(PlannedParagraph {
                            page: page.index,
                            region: Some(r.id),
                            placement: Placement::Image,
                            structure: WordStructure::Text,
                            runs: vec![],
                            image: Some(r.bbox),
                        });
                    } else {
                        exclude("illustrations not included");
                    }
                }
                RegionKind::Table => match settings.include.tables {
                    TablePolicy::Text => {
                        stats.tables += 1;
                        let ps = paragraphs_of(page, r, Placement::TableText, settings, &mut stats);
                        body.extend(ps);
                        warnings.push(format!("page {}: a table was exported as plain text; cell structure is not preserved", page.index + 1));
                    }
                    TablePolicy::Image => {
                        stats.tables += 1;
                        stats.images += 1;
                        body.push(PlannedParagraph {
                            page: page.index,
                            region: Some(r.id),
                            placement: Placement::Image,
                            structure: WordStructure::Text,
                            runs: vec![],
                            image: Some(r.bbox),
                        });
                    }
                    TablePolicy::Skip => exclude("tables skipped"),
                },
                RegionKind::Body | RegionKind::Heading => {
                    let ps = paragraphs_of(page, r, Placement::Body, settings, &mut stats);
                    stats.headings += ps
                        .iter()
                        .filter(|p| p.placement == Placement::Heading)
                        .count() as u32;
                    body.extend(ps);
                }
            }
        }
        // words outside any region are exported as body with a warning
        let stray: Vec<&SpanSnap> = page.spans.iter().filter(|s| s.region.is_none()).collect();
        if !stray.is_empty() {
            warnings.push(format!(
                "page {}: {} words outside any region were exported as body text",
                page.index + 1,
                stray.len()
            ));
            let mut para = PlannedParagraph {
                page: page.index,
                region: None,
                placement: Placement::Body,
                structure: WordStructure::Text,
                runs: vec![],
                image: None,
            };
            for s in stray {
                stats.words += 1;
                para.runs.push(PlannedRun {
                    text: format!(
                        "{}{}",
                        s.text,
                        if s.trailing == "\n" {
                            " "
                        } else {
                            s.trailing.as_str()
                        }
                    ),
                    span: Some(s.id),
                    flag: None,
                });
            }
            body.push(para);
        }
        for s in page
            .spans
            .iter()
            .filter(|s| s.anchors.iter().any(|(p, _)| *p != page.index))
        {
            cross_page_words.push(
                s.text
                    .trim_end_matches(|c: char| !c.is_alphanumeric())
                    .to_string(),
            );
        }
        insert_side_notes(&mut body, notes, page);
        let (header, footer, mut body_all) = if native {
            (head, footers, body)
        } else {
            let mut all = head;
            all.append(&mut body);
            all.append(&mut footnotes);
            all.append(&mut footers);
            (vec![], vec![], all)
        };
        if native {
            body_all.append(&mut footnotes);
        }
        stats.paragraphs += body_all.len() as u32 + header.len() as u32 + footer.len() as u32;
        let last = pi + 1 == n;
        let break_after = match settings.structure {
            PageStructure::Continuous => PageBreak::None,
            PageStructure::Mirror if native => PageBreak::Section,
            PageStructure::Mirror if last => PageBreak::None,
            PageStructure::Mirror => PageBreak::Page,
        };
        pages.push(PlannedPage {
            index: page.index,
            label: page.label.clone(),
            body: body_all,
            header,
            footer,
            break_after,
        });
    }
    Plan {
        pages,
        exclusions,
        warnings,
        stats,
        cross_page_words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sample_snapshot;

    #[test]
    fn styled_policy_places_furniture_in_order() {
        let snap = sample_snapshot(3, false);
        let plan = compose(&snap);
        assert_eq!(plan.pages.len(), 3);
        let p0 = &plan.pages[0];
        assert_eq!(p0.body[0].placement, Placement::RunningHead);
        assert_eq!(p0.body[1].placement, Placement::PrintedPageNumber);
        assert!(p0.body.iter().any(|p| p.placement == Placement::SideNote));
        let side = p0
            .body
            .iter()
            .position(|p| p.placement == Placement::SideNote)
            .unwrap();
        assert_eq!(
            p0.body[side + 1].placement,
            Placement::Body,
            "side note precedes the body paragraph it annotates"
        );
        assert_eq!(p0.body.last().unwrap().placement, Placement::FootnoteText);
        assert_eq!(p0.break_after, PageBreak::Page);
        assert_eq!(plan.pages[2].break_after, PageBreak::None);
        assert!(plan.exclusions.iter().any(|e| e.kind == "catchword"));
        assert!(plan.stats.cross_page_words >= 1);
    }

    #[test]
    fn native_policy_moves_furniture_into_sections() {
        let mut snap = sample_snapshot(2, false);
        snap.settings.furniture = FurniturePolicy::NativeHeadersFooters;
        let plan = compose(&snap);
        let p0 = &plan.pages[0];
        assert!(p0
            .header
            .iter()
            .all(|p| p.placement == Placement::NativeHeader));
        assert!(p0
            .footer
            .iter()
            .any(|p| p.placement == Placement::NativeFooter));
        assert!(p0
            .body
            .iter()
            .all(|p| !matches!(p.placement, Placement::RunningHead | Placement::SideNote)));
        assert_eq!(p0.break_after, PageBreak::Section);
    }
}
