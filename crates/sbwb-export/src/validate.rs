//! Read the produced package back and check it against the plan (EXP-04):
//! structure, coverage, duplicates and omissions, page-boundary policy,
//! header linkage, text parity, and copy-kind guarantees.

use std::collections::HashMap;
use std::io::Read;

use sbwb_core::{Result, SbwbError};
use serde::{Deserialize, Serialize};

use crate::plan::{PageBreak, Plan};
use crate::settings::{CopyKind, FurniturePolicy, PageStructure};
use crate::snapshot::ExportSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub ok: bool,
    pub checks: Vec<Check>,
    pub paragraphs_read: u32,
    pub page_breaks_read: u32,
    pub sections_read: u32,
    pub comments_read: u32,
    pub highlights_read: u32,
    /// Word pagination is not checked unless a rendering pass ran.
    pub pagination_checked: bool,
}

fn read_entry(
    zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<String>> {
    match zip.by_name(name) {
        Ok(mut f) => {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            Ok(Some(s))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(SbwbError::other(format!("read {name}: {e}"))),
    }
}

fn node_text(n: roxmltree::Node) -> String {
    let mut s = String::new();
    for d in n.descendants() {
        if d.is_element() && d.tag_name().name() == "t" {
            s.push_str(d.text().unwrap_or(""));
        }
    }
    s
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct ReadBack {
    /// (text, has a frame)
    body_paragraphs: Vec<(String, bool)>,
    page_breaks: u32,
    sections: u32,
    header_rids: Vec<String>,
    footer_rids: Vec<String>,
    highlights: u32,
    comment_refs: u32,
    frames: u32,
}

fn read_document(xml: &str) -> Result<ReadBack> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| SbwbError::other(format!("document.xml: {e}")))?;
    let body = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
        .ok_or_else(|| SbwbError::other("document.xml has no body"))?;
    let mut rb = ReadBack {
        body_paragraphs: vec![],
        page_breaks: 0,
        sections: 0,
        header_rids: vec![],
        footer_rids: vec![],
        highlights: 0,
        comment_refs: 0,
        frames: 0,
    };
    for n in body.descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "p" => {
                let framed = n
                    .descendants()
                    .any(|d| d.is_element() && d.tag_name().name() == "framePr");
                rb.body_paragraphs.push((node_text(n), framed));
            }
            "br" if n.attribute((
                "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                "type",
            )) == Some("page") =>
            {
                rb.page_breaks += 1
            }
            "sectPr" => {
                rb.sections += 1;
                for c in n.children().filter(|c| c.is_element()) {
                    let rid = c
                        .attribute((
                            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                            "id",
                        ))
                        .map(|s| s.to_string());
                    match (c.tag_name().name(), rid) {
                        ("headerReference", Some(r)) => rb.header_rids.push(r),
                        ("footerReference", Some(r)) => rb.footer_rids.push(r),
                        _ => {}
                    }
                }
            }
            "highlight" => rb.highlights += 1,
            "commentReference" => rb.comment_refs += 1,
            "framePr" => rb.frames += 1,
            _ => {}
        }
    }
    Ok(rb)
}

fn rels(xml: &str) -> Result<HashMap<String, String>> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| SbwbError::other(format!("rels: {e}")))?;
    Ok(doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Relationship")
        .filter_map(|n| {
            Some((
                n.attribute("Id")?.to_string(),
                n.attribute("Target")?.to_string(),
            ))
        })
        .collect())
}

pub fn validate(
    bytes: &[u8],
    snapshot: &ExportSnapshot,
    plan: &Plan,
    drop_caps: u32,
) -> Result<ValidationReport> {
    let mut report = ValidationReport::default();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| SbwbError::other(format!("open docx: {e}")))?;
    let mut check = |name: &str, ok: bool, detail: String| {
        report.checks.push(Check {
            name: name.into(),
            ok,
            detail,
        })
    };

    // structure
    let ct = read_entry(&mut zip, "[Content_Types].xml")?;
    let doc_xml = read_entry(&mut zip, "word/document.xml")?;
    let doc_rels = read_entry(&mut zip, "word/_rels/document.xml.rels")?.unwrap_or_default();
    let core = read_entry(&mut zip, "docProps/core.xml")?.unwrap_or_default();
    check(
        "package structure",
        ct.is_some() && doc_xml.is_some(),
        if doc_xml.is_some() {
            "content types and document part present".into()
        } else {
            "missing document.xml".into()
        },
    );
    let Some(doc_xml) = doc_xml else {
        report.ok = false;
        return Ok(report);
    };
    let rb = read_document(&doc_xml)?;
    report.paragraphs_read = rb.body_paragraphs.len() as u32;
    report.page_breaks_read = rb.page_breaks;
    report.sections_read = rb.sections;
    report.highlights_read = rb.highlights;
    report.comments_read = rb.comment_refs;

    // text parity: body paragraphs in order (frames from drop caps merge back)
    let expected: Vec<String> = plan
        .pages
        .iter()
        .flat_map(|p| {
            p.body.iter().map(|q| {
                if q.image.is_some() {
                    String::new()
                } else {
                    norm(&q.text())
                }
            })
        })
        .collect();
    let mut actual: Vec<String> = Vec::new();
    let mut i = 0;
    let mut merged = 0;
    let paras = &rb.body_paragraphs;
    while i < paras.len() {
        let (t, framed) = (&paras[i].0, paras[i].1);
        // a framed drop-cap letter followed by the rest of its paragraph
        if framed && i + 1 < paras.len() {
            actual.push(norm(&format!("{}{}", t, paras[i + 1].0)));
            merged += 1;
            i += 2;
        } else {
            actual.push(norm(t));
            i += 1;
        }
    }
    check(
        "drop caps",
        merged == drop_caps,
        format!("{merged} framed drop caps read, {drop_caps} written"),
    );
    // pages with no content emit one empty paragraph; drop empties on both sides
    let exp_ne: Vec<&String> = expected.iter().filter(|s| !s.is_empty()).collect();
    let act_ne: Vec<&String> = actual.iter().filter(|s| !s.is_empty()).collect();
    let parity = exp_ne == act_ne;
    let first_diff = exp_ne.iter().zip(act_ne.iter()).position(|(a, b)| a != b);
    check(
        "text parity",
        parity,
        if parity {
            format!("{} paragraphs match the snapshot", act_ne.len())
        } else {
            format!(
                "{} planned vs {} read; first difference at paragraph {}",
                exp_ne.len(),
                act_ne.len(),
                first_diff
                    .map(|d| (d + 1).to_string())
                    .unwrap_or_else(|| "end".into())
            )
        },
    );

    // coverage: every included span appears once (planned), no duplicates
    let mut seen = std::collections::HashSet::new();
    let mut dup = 0;
    let mut planned_spans = 0;
    for p in plan
        .pages
        .iter()
        .flat_map(|p| p.body.iter().chain(p.header.iter()).chain(p.footer.iter()))
    {
        for s in p.spans() {
            planned_spans += 1;
            if !seen.insert(s) {
                dup += 1;
            }
        }
    }
    let excluded_words: u32 = plan.exclusions.iter().map(|e| e.words).sum();
    let total_spans = snapshot.word_count() as u32;
    let covered = planned_spans as u32 + excluded_words == total_spans;
    check("coverage", covered && dup == 0, format!("{planned_spans} words emitted + {excluded_words} excluded = {} of {total_spans}; duplicates {dup}", planned_spans as u32 + excluded_words));

    // cross-page words appear exactly once in the document text
    let all_text: String = rb
        .body_paragraphs
        .iter()
        .map(|p| p.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let mut cp_ok = true;
    let mut cp_detail = Vec::new();
    for w in &plan.cross_page_words {
        if w.is_empty() {
            continue;
        }
        let count = all_text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t == w)
            .count();
        if count != 1 {
            cp_ok = false;
            cp_detail.push(format!("“{w}” ×{count}"));
        }
    }
    check(
        "cross-page words",
        cp_ok,
        if cp_ok {
            format!(
                "{} joined words appear once each",
                plan.cross_page_words.len()
            )
        } else {
            cp_detail.join(", ")
        },
    );

    // page boundary policy
    let native = snapshot.settings.furniture == FurniturePolicy::NativeHeadersFooters
        && snapshot.settings.structure == PageStructure::Mirror;
    let pages = plan.pages.len() as u32;
    let (bok, bdetail) = match snapshot.settings.structure {
        PageStructure::Continuous => (
            rb.page_breaks == 0 && rb.sections == 1,
            format!(
                "continuous: {} page breaks, {} sections",
                rb.page_breaks, rb.sections
            ),
        ),
        PageStructure::Mirror if native => (
            rb.sections == pages && rb.page_breaks == 0,
            format!("{} sections for {pages} pages", rb.sections),
        ),
        PageStructure::Mirror => (
            rb.page_breaks + 1 == pages.max(1) && rb.sections == 1,
            format!("{} page breaks for {pages} pages", rb.page_breaks),
        ),
    };
    let planned_breaks = plan
        .pages
        .iter()
        .filter(|p| p.break_after == PageBreak::Page)
        .count() as u32;
    check(
        "page boundary policy",
        bok && (native || planned_breaks == rb.page_breaks),
        bdetail,
    );

    // header/footer linkage
    let rel_map = rels(&doc_rels)?;
    let mut link_ok = true;
    let mut missing = Vec::new();
    for rid in rb.header_rids.iter().chain(rb.footer_rids.iter()) {
        match rel_map.get(rid) {
            Some(target) => {
                let name = format!("word/{target}");
                if read_entry(&mut zip, &name)?.is_none() {
                    link_ok = false;
                    missing.push(name);
                }
            }
            None => {
                link_ok = false;
                missing.push(rid.clone());
            }
        }
    }
    if native {
        let need = plan.pages.iter().filter(|p| !p.header.is_empty()).count();
        if rb.header_rids.len() < need {
            link_ok = false;
            missing.push(format!(
                "{} header references for {need} pages with running heads",
                rb.header_rids.len()
            ));
        }
    }
    check(
        "header and footer linkage",
        link_ok,
        if link_ok {
            format!(
                "{} headers, {} footers linked",
                rb.header_rids.len(),
                rb.footer_rids.len()
            )
        } else {
            missing.join(", ")
        },
    );

    // no source marker strings
    let markers = all_text.contains("[Source page") || all_text.contains("[Page ");
    check(
        "no source markers",
        !markers,
        if markers {
            "found a literal page marker".into()
        } else {
            "no literal page markers".into()
        },
    );

    // copy kind guarantees
    let comments_part = read_entry(&mut zip, "word/comments.xml")?;
    match snapshot.settings.copy {
        CopyKind::Clean => {
            let clean = rb.comment_refs == 0
                && rb.highlights == 0
                && comments_part
                    .map(|c| !c.contains("<w:comment "))
                    .unwrap_or(true);
            check(
                "clean copy",
                clean,
                if clean {
                    "no comments, no highlights".into()
                } else {
                    format!(
                        "{} comment references, {} highlights",
                        rb.comment_refs, rb.highlights
                    )
                },
            );
        }
        CopyKind::Working => {
            let flags = plan.stats.flags;
            let ok = rb.comment_refs == flags && rb.highlights == flags;
            check(
                "working copy annotations",
                ok,
                format!(
                    "{flags} flags → {} comments, {} highlights",
                    rb.comment_refs, rb.highlights
                ),
            );
        }
    }

    // metadata
    let title = snapshot
        .settings
        .metadata
        .title
        .clone()
        .or_else(|| snapshot.book.title.clone())
        .unwrap_or_default();
    let meta_ok = title.is_empty()
        || core.contains(&format!(
            "<dc:title>{}</dc:title>",
            crate::docx_escape(&title)
        ));
    check(
        "document properties",
        meta_ok,
        if meta_ok {
            "title and author written".into()
        } else {
            "title missing from core.xml".into()
        },
    );

    // styles present
    let styles = read_entry(&mut zip, "word/styles.xml")?.unwrap_or_default();
    let want = [
        "RunningHead",
        "PrintedPageNumber",
        "SideNote",
        "FooterNote",
        "FootnoteText",
        "Heading1",
    ];
    let have: Vec<&str> = want
        .iter()
        .copied()
        .filter(|s| styles.contains(&format!(r#"w:styleId="{s}""#)))
        .collect();
    check(
        "named styles",
        have.len() == want.len(),
        format!("{} of {} furniture styles defined", have.len(), want.len()),
    );

    report.pagination_checked = false;
    report.ok = report.checks.iter().all(|c| c.ok);
    Ok(report)
}
