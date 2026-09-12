//! Companion archive (EXP-05): machine-readable transcript with source
//! map, PAGE XML per page, the immutable snapshot, the manifest, and the
//! validation report.

use std::io::Write;

use sbwb_core::{Result, SbwbError};
use serde::Serialize;

use crate::plan::Plan;
use crate::snapshot::ExportSnapshot;
use crate::validate::ValidationReport;
use crate::ExportReport;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Minimal PAGE XML (PRImA schema 2019) for one page: regions with
/// coordinates in pixels at 300 dpi and their text in reading order.
pub fn page_xml(snapshot: &ExportSnapshot, page: &crate::snapshot::PageSnap) -> String {
    let px = 300.0 / 72.0;
    let w = (page.width_pt * px).round() as u32;
    let h = (page.height_pt * px).round() as u32;
    let mut s = String::new();
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    s.push_str(
        r#"<PcGts xmlns="http://schema.primaresearch.org/PAGE/gts/pagecontent/2019-07-15">"#,
    );
    s.push_str(&format!("<Metadata><Creator>SBWB {}</Creator><Created>{}</Created><LastChange>{}</LastChange></Metadata>", esc(&snapshot.app_version), esc(&snapshot.created_at), esc(&snapshot.created_at)));
    s.push_str(&format!(
        r#"<Page imageFilename="{}#page={}" imageWidth="{w}" imageHeight="{h}">"#,
        esc(&snapshot.book.source_name),
        page.index + 1
    ));
    let mut regions: Vec<&crate::snapshot::RegionSnap> = page.regions.iter().collect();
    regions.sort_by_key(|r| r.order);
    s.push_str("<ReadingOrder><OrderedGroup id=\"ro\">");
    for (i, r) in regions.iter().enumerate() {
        s.push_str(&format!(
            r#"<RegionRefIndexed index="{i}" regionRef="r_{}"/>"#,
            r.id
        ));
    }
    s.push_str("</OrderedGroup></ReadingOrder>");
    for r in regions {
        let kind = match r.kind {
            sbwb_layout::RegionKind::Illustration => "ImageRegion",
            sbwb_layout::RegionKind::Table => "TableRegion",
            _ => "TextRegion",
        };
        let ty = match r.kind {
            sbwb_layout::RegionKind::Header => " type=\"header\"",
            sbwb_layout::RegionKind::Footer => " type=\"footer\"",
            sbwb_layout::RegionKind::Marginalia => " type=\"marginalia\"",
            sbwb_layout::RegionKind::Footnote => " type=\"footnote\"",
            sbwb_layout::RegionKind::PageNumber => " type=\"page-number\"",
            sbwb_layout::RegionKind::Heading => " type=\"heading\"",
            sbwb_layout::RegionKind::Catchword => " type=\"catch-word\"",
            sbwb_layout::RegionKind::Body => " type=\"paragraph\"",
            _ => "",
        };
        let b = &r.bbox;
        let pts = format!(
            "{},{} {},{} {},{} {},{}",
            (b.x * px) as i64,
            (b.y * px) as i64,
            ((b.x + b.w) * px) as i64,
            (b.y * px) as i64,
            ((b.x + b.w) * px) as i64,
            ((b.y + b.h) * px) as i64,
            (b.x * px) as i64,
            ((b.y + b.h) * px) as i64
        );
        s.push_str(&format!(
            r#"<{kind} id="r_{}"{ty}><Coords points="{pts}"/>"#,
            r.id
        ));
        if kind == "TextRegion" {
            let text: String = page
                .spans
                .iter()
                .filter(|sp| sp.region == Some(r.id))
                .map(|sp| format!("{}{}", sp.text, sp.trailing))
                .collect();
            s.push_str(&format!(
                "<TextEquiv><Unicode>{}</Unicode></TextEquiv>",
                esc(text.trim())
            ));
        }
        s.push_str(&format!("</{kind}>"));
    }
    s.push_str("</Page></PcGts>");
    s
}

#[derive(Serialize)]
struct TranscriptSpan<'a> {
    id: String,
    page: u32,
    region: Option<String>,
    text: &'a str,
    trailing: &'a str,
    origin: &'a sbwb_text::SpanOrigin,
    confidence: Option<f32>,
    paragraph_start: bool,
    anchors: &'a [(u32, sbwb_core::Rect)],
    /// Text with no scan anchor is explicit (EXP-05).
    unanchored: bool,
    flag: Option<&'a crate::snapshot::FlagSnap>,
}

/// Write the bundle next to the document: `<stem>.sbwb-archive.zip`.
pub fn write(
    path: &std::path::Path,
    snapshot: &ExportSnapshot,
    plan: &Plan,
    report: &ExportReport,
    validation: &ValidationReport,
) -> Result<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let z = |e: zip::result::ZipError| SbwbError::other(format!("archive: {e}"));
    let spans: Vec<TranscriptSpan> = snapshot
        .pages
        .iter()
        .flat_map(|p| {
            p.spans.iter().map(move |s| TranscriptSpan {
                id: s.id.to_string(),
                page: p.index,
                region: s.region.map(|r| r.to_string()),
                text: &s.text,
                trailing: &s.trailing,
                origin: &s.origin,
                confidence: s.confidence,
                paragraph_start: s.paragraph_start,
                anchors: &s.anchors,
                unanchored: s.anchors.is_empty(),
                flag: s.flag.as_ref(),
            })
        })
        .collect();
    zip.start_file("transcript.json", opts).map_err(z)?;
    zip.write_all(
        serde_json::to_string_pretty(
            &serde_json::json!({ "export": snapshot.id, "spans": spans }),
        )?
        .as_bytes(),
    )?;
    zip.start_file("plan.json", opts).map_err(z)?;
    zip.write_all(serde_json::to_string_pretty(plan)?.as_bytes())?;
    zip.start_file("snapshot.json", opts).map_err(z)?;
    zip.write_all(serde_json::to_string_pretty(snapshot)?.as_bytes())?;
    zip.start_file("manifest.json", opts).map_err(z)?;
    zip.write_all(serde_json::to_string_pretty(report)?.as_bytes())?;
    zip.start_file("validation.json", opts).map_err(z)?;
    zip.write_all(serde_json::to_string_pretty(validation)?.as_bytes())?;
    for p in &snapshot.pages {
        zip.start_file(format!("layout/page-{:04}.xml", p.index + 1), opts)
            .map_err(z)?;
        zip.write_all(page_xml(snapshot, p).as_bytes())?;
    }
    zip.finish().map_err(z)?;
    Ok(())
}
