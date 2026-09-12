//! Word export (EXP-01..06): snapshot → composition plan → DOCX package →
//! validation → atomic publish (+ archive bundle).

pub mod archive;
pub mod docx;
pub mod fonts;
pub mod plan;
pub mod settings;
pub mod snapshot;
pub mod validate;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sbwb_core::{Rect, Result, SbwbError};
use serde::{Deserialize, Serialize};

pub use plan::{compose, Exclusion, Placement, Plan, PlanStats};
pub use settings::{
    CopyKind, ExportSettings, FormatPreset, FurniturePolicy, Inclusion, PageStructure, TablePolicy,
};
pub use snapshot::{ExportSnapshot, PageSnap, RegionSnap, SpanSnap};
pub use validate::ValidationReport;

pub(crate) fn docx_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Supplies scan crops for illustrations; the app backs it with the render
/// worker, tests with nothing.
pub trait RenderProvider {
    fn crop_png(&self, page: u32, bbox: &Rect, dpi: u32) -> Result<Vec<u8>>;
}

pub struct NoRenders;
impl RenderProvider for NoRenders {
    fn crop_png(&self, _page: u32, _bbox: &Rect, _dpi: u32) -> Result<Vec<u8>> {
        Err(SbwbError::other("no renderer available"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Compose,
    Package,
    Validate,
    Publish,
    Archive,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReport {
    pub id: String,
    pub created_at: String,
    pub copy: CopyKind,
    pub settings: ExportSettings,
    pub stats: PlanStats,
    pub exclusions: Vec<Exclusion>,
    pub warnings: Vec<String>,
    pub comments: u32,
    pub highlights: u32,
    pub images: u32,
    pub drop_caps: u32,
    pub cross_page_policy: String,
    pub checksum_blake3: String,
    pub docx_path: PathBuf,
    pub archive_path: Option<PathBuf>,
    pub validation: ValidationReport,
    pub elapsed_ms: u64,
}

/// Gate for the clean copy (EXP-01): every page in scope currently approved.
pub fn clean_copy_blockers(snapshot: &ExportSnapshot) -> Vec<u32> {
    snapshot.pages_not_approved()
}

/// Run an export end to end. The document is written to `<dest>.partial`
/// and renamed only after validation passes, so a failure or cancel never
/// leaves a file that looks finished (EXP-04).
pub fn run(
    snapshot: &ExportSnapshot,
    dest: &Path,
    renders: &dyn RenderProvider,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Phase, u32, u32),
) -> Result<ExportReport> {
    let started = std::time::Instant::now();
    if snapshot.settings.copy == CopyKind::Clean {
        let blockers = clean_copy_blockers(snapshot);
        if !blockers.is_empty() {
            return Err(SbwbError::Conflict(format!(
                "clean copy needs every page approved: {} page{} left",
                blockers.len(),
                if blockers.len() == 1 { "" } else { "s" }
            )));
        }
    }
    let check_cancel = |c: &AtomicBool| -> Result<()> {
        if c.load(Ordering::SeqCst) {
            Err(SbwbError::other("export cancelled"))
        } else {
            Ok(())
        }
    };
    progress(Phase::Compose, 0, 1);
    let mut plan = compose(snapshot);
    if let Some(w) = fonts::warning_for(&snapshot.settings.preset.body_font) {
        plan.warnings.push(w);
    }
    if snapshot.settings.preset.heading_font != snapshot.settings.preset.body_font {
        if let Some(w) = fonts::warning_for(&snapshot.settings.preset.heading_font) {
            plan.warnings.push(w);
        }
    }
    progress(Phase::Compose, 1, 1);
    check_cancel(cancel)?;

    progress(Phase::Package, 0, plan.pages.len() as u32);
    let emitted = docx::write(snapshot, &plan, renders)?;
    plan.warnings.extend(emitted.image_failures.iter().cloned());
    progress(
        Phase::Package,
        plan.pages.len() as u32,
        plan.pages.len() as u32,
    );
    check_cancel(cancel)?;

    progress(Phase::Validate, 0, 1);
    let validation = validate::validate(&emitted.bytes, snapshot, &plan, emitted.drop_caps)?;
    progress(Phase::Validate, 1, 1);
    if !validation.ok {
        let failed: Vec<String> = validation
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| format!("{}: {}", c.name, c.detail))
            .collect();
        return Err(SbwbError::other(format!(
            "validation failed · {}",
            failed.join(" · ")
        )));
    }
    check_cancel(cancel)?;

    progress(Phase::Publish, 0, 1);
    let partial = dest.with_extension("docx.partial");
    std::fs::write(&partial, &emitted.bytes)?;
    let checksum = blake3::hash(&emitted.bytes).to_hex().to_string();
    std::fs::rename(&partial, dest)?;
    progress(Phase::Publish, 1, 1);

    let mut report = ExportReport {
        id: snapshot.id.clone(),
        created_at: snapshot.created_at.clone(),
        copy: snapshot.settings.copy,
        settings: snapshot.settings.clone(),
        stats: plan.stats.clone(),
        exclusions: plan.exclusions.clone(),
        warnings: plan.warnings.clone(),
        comments: emitted.comments,
        highlights: emitted.highlights,
        images: emitted.images,
        drop_caps: emitted.drop_caps,
        cross_page_policy: "a word joined across a page boundary belongs to the earlier page; the boundary follows it and the consumed continuation is not repeated".into(),
        checksum_blake3: checksum,
        docx_path: dest.to_path_buf(),
        archive_path: None,
        validation: validation.clone(),
        elapsed_ms: 0,
    };
    let report_path = dest.with_extension("export-report.json");
    if snapshot.settings.archive {
        progress(Phase::Archive, 0, 1);
        let archive_path = dest.with_extension("sbwb-archive.zip");
        archive::write(&archive_path, snapshot, &plan, &report, &validation)?;
        report.archive_path = Some(archive_path);
        progress(Phase::Archive, 1, 1);
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    progress(Phase::Done, 1, 1);
    Ok(report)
}

#[cfg(any(test, feature = "test-support", debug_assertions))]
pub mod test_support {
    //! Synthetic snapshots for tests and the export fixture (M7.10).
    use super::*;
    use sbwb_core::{RegionId, SpanId};
    use sbwb_layout::{RegionKind, WordStructure};
    use sbwb_text::SpanOrigin;
    use snapshot::{BookInfo, FlagSnap, PageSnap, RegionSnap, SpanSnap};

    fn region(kind: RegionKind, order: u32, x: f64, y: f64, w: f64, h: f64) -> RegionSnap {
        RegionSnap {
            id: RegionId::new(),
            kind,
            order,
            structure: WordStructure::Text,
            bbox: Rect { x, y, w, h },
            anchor_y: Some(y),
        }
    }

    fn words(
        page: u32,
        region: &RegionSnap,
        text: &str,
        start_y: f64,
        flag_every: usize,
    ) -> Vec<SpanSnap> {
        let mut out = Vec::new();
        let mut x = region.bbox.x;
        let mut y = start_y;
        for (i, w) in text.split_whitespace().enumerate() {
            let width = w.len() as f64 * 5.0;
            if x + width > region.bbox.x + region.bbox.w {
                x = region.bbox.x;
                y += 12.0;
            }
            let flag = if flag_every > 0 && i % flag_every == flag_every - 1 {
                Some(FlagSnap {
                    kind: "conflicting_readings".into(),
                    score: Some(40),
                    reason: "low OCR confidence".into(),
                    deferred: false,
                })
            } else {
                None
            };
            out.push(SpanSnap {
                id: SpanId::new(),
                region: Some(region.id),
                text: w.to_string(),
                trailing: " ".into(),
                origin: SpanOrigin::Ocr,
                confidence: Some(90.0),
                paragraph_start: i == 0,
                structure: WordStructure::Text,
                anchors: vec![(
                    page,
                    Rect {
                        x,
                        y,
                        w: width,
                        h: 10.0,
                    },
                )],
                flag,
            });
            x += width + 4.0;
        }
        if let Some(last) = out.last_mut() {
            last.trailing = "\n".into();
        }
        out
    }

    /// `n` pages with running head, page number, a side note, two body
    /// paragraphs, a footnote, a catchword, and a word joined across the
    /// boundary between pages 1 and 2. `big` makes the body ~500 words.
    pub fn sample_snapshot(n: u32, big: bool) -> ExportSnapshot {
        let mut pages = Vec::new();
        let filler = if big {
            (0..70)
                .map(|i| format!("word{i} lorem ipsum dolor sit amet consectetur"))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            "The Indian trade was soon engrossed by the Phenicians, who for many ages continued in undisturbed possession of the monopoly.".to_string()
        };
        for i in 0..n {
            let head = region(RegionKind::Header, 0, 60.0, 30.0, 200.0, 12.0);
            let num = region(RegionKind::PageNumber, 1, 280.0, 30.0, 30.0, 12.0);
            let body1 = region(RegionKind::Body, 2, 60.0, 60.0, 240.0, 200.0);
            let note = region(RegionKind::Marginalia, 3, 10.0, 150.0, 45.0, 30.0);
            let body2 = region(RegionKind::Body, 4, 60.0, 270.0, 240.0, 200.0);
            let foot = region(RegionKind::Footnote, 5, 60.0, 480.0, 240.0, 30.0);
            let catch = region(RegionKind::Catchword, 6, 260.0, 520.0, 40.0, 12.0);
            let mut spans = Vec::new();
            spans.extend(words(
                i,
                &head,
                if i % 2 == 0 {
                    "HISTORY OF CHRISTIANITY"
                } else {
                    "IN INDIA: BOOK I."
                },
                30.0,
                0,
            ));
            spans.extend(words(i, &num, &format!("{}", i + 6), 30.0, 0));
            spans.extend(words(
                i,
                &body1,
                &format!("Chapter text page {}. {filler}", i + 1),
                60.0,
                7,
            ));
            spans.extend(words(i, &note, &format!("A.D. {}", 1500 + i), 150.0, 0));
            spans.extend(words(
                i,
                &body2,
                &format!("Second paragraph on page {}. {filler} carried on", i + 1),
                270.0,
                0,
            ));
            spans.extend(words(
                i,
                &foot,
                &format!("* Note {} in the margin of history.", i + 1),
                480.0,
                0,
            ));
            spans.extend(words(i, &catch, "Next", 520.0, 0));
            let mut page = PageSnap {
                index: i,
                label: Some(format!("{}", i + 6)),
                width_pt: 355.0,
                height_pt: 606.0,
                approval: "current".into(),
                approved_outstanding: 0,
                unresolved: 0,
                deferred: 0,
                text_done: true,
                regions: vec![head, num, body1, note, body2, foot, catch],
                spans,
            };
            if i == 0 && n > 1 {
                // "communica-" + "tion" joined: whole word on page 0 with two anchors
                let last_body = page
                    .spans
                    .iter()
                    .rposition(|s| s.region == Some(page.regions[4].id))
                    .unwrap();
                page.spans[last_body].text = "communication".into();
                page.spans[last_body].anchors.push((
                    1,
                    Rect {
                        x: 60.0,
                        y: 60.0,
                        w: 20.0,
                        h: 10.0,
                    },
                ));
                page.spans[last_body].origin = SpanOrigin::AutoApplied;
            }
            pages.push(page);
        }
        ExportSnapshot {
            id: "test-export".into(),
            created_at: "2026-09-12T00:00:00Z".into(),
            app_version: "test".into(),
            book: BookInfo {
                title: Some("The History of Christianity in India".into()),
                author: Some("James Hough".into()),
                source_name: "hough.pdf".into(),
                source_pages: n,
                source_blake3: "0".repeat(64),
                pdf_title: None,
                pdf_author: None,
            },
            settings: ExportSettings::default(),
            pages,
        }
    }
}
