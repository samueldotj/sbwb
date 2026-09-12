//! DOCX writer (EXP-01, EXP-02, EXP-03, EXP-06) over `docx-rs`, plus the
//! package post-pass that sets core properties and per-section page
//! geometry which the builder does not expose.

use std::io::{Read, Write};

use docx_rs::{
    AlignmentType, BreakType, Comment, Docx, Footer, FrameProperty, Header, LineSpacing,
    PageMargin, Paragraph, Pic, Run, RunFonts, Section, Style, StyleType,
};
use sbwb_core::{Rect, Result, SbwbError};

use crate::plan::{PageBreak, Placement, Plan, PlannedParagraph};
use crate::settings::{CopyKind, ExportSettings};
use crate::snapshot::ExportSnapshot;
use crate::RenderProvider;

const TWIP: f64 = 20.0; // per point
const EMU_PER_PT: f64 = 12700.0;

/// Named styles (EXP-03): visually distinct, editable, searchable.
fn styles(settings: &ExportSettings) -> Vec<Style> {
    let p = &settings.preset;
    let body_half = (p.body_size_pt * 2.0).round() as usize;
    let note_half = (p.note_size_pt * 2.0).round() as usize;
    let body_font = RunFonts::new()
        .ascii(p.body_font.clone())
        .hi_ansi(p.body_font.clone());
    let head_font = RunFonts::new()
        .ascii(p.heading_font.clone())
        .hi_ansi(p.heading_font.clone());
    let spacing = |after_pt: f64| {
        LineSpacing::new()
            .after((after_pt * TWIP) as u32)
            .line((240.0 * p.line_spacing) as i32)
    };
    vec![
        Style::new("SbwbBody", StyleType::Paragraph)
            .name("Body text (SBWB)")
            .fonts(body_font.clone())
            .size(body_half)
            .line_spacing(spacing(p.paragraph_spacing_pt)),
        Style::new("Heading1", StyleType::Paragraph)
            .name("heading 1")
            .based_on("SbwbBody")
            .fonts(head_font.clone())
            .size(body_half + 8)
            .bold()
            .outline_lvl(0)
            .line_spacing(LineSpacing::new().before(240).after(120)),
        Style::new("Heading2", StyleType::Paragraph)
            .name("heading 2")
            .based_on("SbwbBody")
            .fonts(head_font.clone())
            .size(body_half + 4)
            .bold()
            .outline_lvl(1)
            .line_spacing(LineSpacing::new().before(200).after(100)),
        Style::new("Heading3", StyleType::Paragraph)
            .name("heading 3")
            .based_on("SbwbBody")
            .fonts(head_font)
            .size(body_half + 2)
            .bold()
            .italic()
            .outline_lvl(2)
            .line_spacing(LineSpacing::new().before(160).after(80)),
        Style::new("RunningHead", StyleType::Paragraph)
            .name("Running head")
            .based_on("SbwbBody")
            .size(note_half)
            .align(AlignmentType::Center)
            .line_spacing(LineSpacing::new().after(60))
            .color("555555"),
        Style::new("PrintedPageNumber", StyleType::Paragraph)
            .name("Printed page number")
            .based_on("SbwbBody")
            .size(note_half)
            .align(AlignmentType::Right)
            .line_spacing(LineSpacing::new().after(120))
            .color("555555"),
        Style::new("SideNote", StyleType::Paragraph)
            .name("Side note")
            .based_on("SbwbBody")
            .size(note_half)
            .italic()
            .indent(Some(360), None, Some(360), None)
            .line_spacing(LineSpacing::new().after(60))
            .color("444444"),
        Style::new("FooterNote", StyleType::Paragraph)
            .name("Footer note")
            .based_on("SbwbBody")
            .size(note_half)
            .align(AlignmentType::Center)
            .line_spacing(LineSpacing::new().before(120).after(60))
            .color("555555"),
        Style::new("FootnoteText", StyleType::Paragraph)
            .name("Footnote text")
            .based_on("SbwbBody")
            .size(note_half)
            .line_spacing(LineSpacing::new().after(40)),
        Style::new("TableText", StyleType::Paragraph)
            .name("Table text")
            .based_on("SbwbBody")
            .size(note_half)
            .fonts(RunFonts::new().ascii("Consolas").hi_ansi("Consolas"))
            .line_spacing(LineSpacing::new().after(0)),
        Style::new("SbwbFlag", StyleType::Character)
            .name("Review flag")
            .highlight("yellow"),
    ]
}

pub struct Emitted {
    pub bytes: Vec<u8>,
    pub comments: u32,
    pub highlights: u32,
    pub images: u32,
    pub image_failures: Vec<String>,
    pub drop_caps: u32,
}

struct Ctx<'a> {
    settings: &'a ExportSettings,
    renders: &'a dyn RenderProvider,
    comment_id: usize,
    comments: u32,
    highlights: u32,
    images: u32,
    image_failures: Vec<String>,
    drop_caps: u32,
    now: String,
}

fn style_for(p: &PlannedParagraph) -> &'static str {
    match (p.placement, p.structure) {
        (Placement::Heading, sbwb_layout::WordStructure::Heading2) => "Heading2",
        (Placement::Heading, sbwb_layout::WordStructure::Heading3) => "Heading3",
        (Placement::Heading, _) => "Heading1",
        (pl, _) => pl.style_id(),
    }
}

impl<'a> Ctx<'a> {
    fn paragraph(&mut self, p: &PlannedParagraph, page_w: f64, first_body: bool) -> Vec<Paragraph> {
        if let Some(bbox) = &p.image {
            return vec![self.image_paragraph(p.page, bbox, page_w)];
        }
        let mut out = Vec::new();
        let mut para = Paragraph::new().style(style_for(p));
        let mut runs = p.runs.iter().peekable();
        // Drop cap: the first letter of the first body paragraph of a page
        // becomes its own framed paragraph (Word's drop-cap construction).
        if first_body && p.placement == Placement::Body && self.settings.preset.drop_cap.enabled {
            if let Some(first) = runs.peek() {
                if first.flag.is_none() {
                    let mut chars = first.text.chars();
                    if let Some(c) = chars.next().filter(|c| c.is_alphabetic()) {
                        let lines = self.settings.preset.drop_cap.lines.clamp(2, 5) as f64;
                        let size_pt = self.settings.preset.body_size_pt
                            * self.settings.preset.line_spacing
                            * lines
                            * 0.92;
                        let frame = FrameProperty::new()
                            .wrap("around")
                            .v_anchor("text")
                            .h_anchor("text")
                            .h_rule("exact")
                            .height((size_pt * TWIP * 1.05) as u32)
                            .h_space(60);
                        let mut cap = Paragraph::new().style("SbwbBody").add_run(
                            Run::new()
                                .add_text(c.to_string())
                                .size((size_pt * 2.0) as usize),
                        );
                        cap.property = cap.property.frame_property(frame);
                        out.push(cap);
                        self.drop_caps += 1;
                        let rest: String = chars.collect();
                        let first = runs.next().unwrap();
                        para = self.push_run(
                            para,
                            &rest,
                            first.span.is_some(),
                            first.flag.as_ref(),
                            p.page,
                        );
                    }
                }
            }
        }
        for r in runs {
            para = self.push_run(para, &r.text, r.span.is_some(), r.flag.as_ref(), p.page);
        }
        out.push(para);
        out
    }

    fn push_run(
        &mut self,
        para: Paragraph,
        text: &str,
        _anchored: bool,
        flag: Option<&crate::snapshot::FlagSnap>,
        page: u32,
    ) -> Paragraph {
        let mut run = Run::new().add_text(text);
        match flag {
            Some(f) if self.settings.copy == CopyKind::Working => {
                run = run.highlight("yellow");
                self.highlights += 1;
                self.comment_id += 1;
                self.comments += 1;
                let note = format!(
                    "{}{}: {}{}",
                    if f.deferred { "Deferred · " } else { "" },
                    f.kind.replace('_', " "),
                    f.reason,
                    f.score.map(|s| format!(" (score {s})")).unwrap_or_default()
                );
                let comment = Comment::new(self.comment_id)
                    .author("SBWB")
                    .date(self.now.clone())
                    .add_paragraph(
                        Paragraph::new()
                            .add_run(Run::new().add_text(format!("Page {} · {note}", page + 1))),
                    );
                para.add_comment_start(comment)
                    .add_run(run)
                    .add_comment_end(self.comment_id)
            }
            _ => para.add_run(run),
        }
    }

    fn image_paragraph(&mut self, page: u32, bbox: &Rect, page_w: f64) -> Paragraph {
        match self.renders.crop_png(page, bbox, 150) {
            Ok(png) if !png.is_empty() => {
                let text_w = (self.settings.preset.page_size_pt().0
                    - (self.settings.preset.margin_left_mm + self.settings.preset.margin_right_mm)
                        * 72.0
                        / 25.4)
                    .max(100.0);
                let scale = if bbox.w > text_w {
                    text_w / bbox.w
                } else {
                    1.0
                };
                let w = (bbox.w * scale * EMU_PER_PT) as u32;
                let h = (bbox.h * scale * EMU_PER_PT) as u32;
                let _ = page_w;
                self.images += 1;
                Paragraph::new()
                    .style("SbwbBody")
                    .align(AlignmentType::Center)
                    .add_run(Run::new().add_image(Pic::new(&png).size(w, h)))
            }
            Ok(_) | Err(_) => {
                self.image_failures.push(format!(
                    "page {}: illustration could not be rendered",
                    page + 1
                ));
                Paragraph::new()
                    .style("SbwbBody")
                    .add_run(Run::new().add_text("[illustration not available]").italic())
            }
        }
    }
}

/// Build the package for a plan.
pub fn write(
    snapshot: &ExportSnapshot,
    plan: &Plan,
    renders: &dyn RenderProvider,
) -> Result<Emitted> {
    let settings = &snapshot.settings;
    let p = &settings.preset;
    let (pw, ph) = p.page_size_pt();
    let margin = PageMargin::new()
        .top((p.margin_top_mm * 72.0 / 25.4 * TWIP) as i32)
        .bottom((p.margin_bottom_mm * 72.0 / 25.4 * TWIP) as i32)
        .left((p.margin_left_mm * 72.0 / 25.4 * TWIP) as i32)
        .right((p.margin_right_mm * 72.0 / 25.4 * TWIP) as i32)
        .header(600)
        .footer(600);
    let mut docx = Docx::new()
        .page_size((pw * TWIP) as u32, (ph * TWIP) as u32)
        .page_margin(margin)
        .default_fonts(
            RunFonts::new()
                .ascii(p.body_font.clone())
                .hi_ansi(p.body_font.clone()),
        )
        .default_size((p.body_size_pt * 2.0) as usize);
    for s in styles(settings) {
        docx = docx.add_style(s);
    }
    let mut ctx = Ctx {
        settings,
        renders,
        comment_id: 0,
        comments: 0,
        highlights: 0,
        images: 0,
        image_failures: vec![],
        drop_caps: 0,
        now: jiff::Timestamp::now()
            .strftime("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
    };
    let n = plan.pages.len();
    for (i, page) in plan.pages.iter().enumerate() {
        let page_w = snapshot.page(page.index).map(|p| p.width_pt).unwrap_or(pw);
        let mut paras: Vec<Paragraph> = Vec::new();
        let mut first_body = true;
        for pp in &page.body {
            let is_body = pp.placement == Placement::Body;
            paras.extend(ctx.paragraph(pp, page_w, first_body && is_body));
            if is_body {
                first_body = false;
            }
        }
        if paras.is_empty() {
            paras.push(Paragraph::new().style("SbwbBody"));
        }
        match page.break_after {
            PageBreak::Page => {
                let last = paras.pop().unwrap();
                paras.push(last.add_run(Run::new().add_break(BreakType::Page)));
            }
            PageBreak::None => {}
            PageBreak::Section => {}
        }
        if page.break_after == PageBreak::Section && i + 1 < n {
            let mut section = Section::new();
            for para in paras {
                section = section.add_paragraph(para);
            }
            let mut header = Header::new();
            for hp in &page.header {
                for para in ctx.paragraph(hp, page_w, false) {
                    header = header.add_paragraph(para);
                }
            }
            let mut footer = Footer::new();
            for fp in &page.footer {
                for para in ctx.paragraph(fp, page_w, false) {
                    footer = footer.add_paragraph(para);
                }
            }
            docx = docx.add_section(section.header(header).footer(footer));
        } else {
            for para in paras {
                docx = docx.add_paragraph(para);
            }
            if page.break_after == PageBreak::Section {
                // Last page: the document-level section carries its furniture.
                let mut header = Header::new();
                for hp in &page.header {
                    for para in ctx.paragraph(hp, page_w, false) {
                        header = header.add_paragraph(para);
                    }
                }
                let mut footer = Footer::new();
                for fp in &page.footer {
                    for para in ctx.paragraph(fp, page_w, false) {
                        footer = footer.add_paragraph(para);
                    }
                }
                docx = docx.header(header).footer(footer);
            }
        }
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buf)
        .map_err(|e| SbwbError::other(format!("pack docx: {e}")))?;
    let bytes = post_process(buf.into_inner(), snapshot, plan)?;
    Ok(Emitted {
        bytes,
        comments: ctx.comments,
        highlights: ctx.highlights,
        images: ctx.images,
        image_failures: ctx.image_failures,
        drop_caps: ctx.drop_caps,
    })
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Rewrite the package: core properties (title, author, subject, dates)
/// and uniform page size, margins, and next-page type on every section.
fn post_process(bytes: Vec<u8>, snapshot: &ExportSnapshot, plan: &Plan) -> Result<Vec<u8>> {
    let settings = &snapshot.settings;
    let p = &settings.preset;
    let (pw, ph) = p.page_size_pt();
    let pg_sz = format!(
        r#"<w:pgSz w:w="{}" w:h="{}" />"#,
        (pw * TWIP) as u32,
        (ph * TWIP) as u32
    );
    let pg_mar = format!(
        r#"<w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}" w:header="600" w:footer="600" w:gutter="0" />"#,
        (p.margin_top_mm * 72.0 / 25.4 * TWIP) as i32,
        (p.margin_right_mm * 72.0 / 25.4 * TWIP) as i32,
        (p.margin_bottom_mm * 72.0 / 25.4 * TWIP) as i32,
        (p.margin_left_mm * 72.0 / 25.4 * TWIP) as i32
    );
    let title = settings
        .metadata
        .title
        .clone()
        .or_else(|| snapshot.book.title.clone())
        .unwrap_or_default();
    let author = settings
        .metadata
        .author
        .clone()
        .or_else(|| snapshot.book.author.clone())
        .unwrap_or_default();
    let subject = settings.metadata.subject.clone().unwrap_or_default();
    let now = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:subject>{}</dc:subject><dc:creator>{}</dc:creator><cp:keywords>SBWB export {}</cp:keywords><dc:description>{}</dc:description><cp:lastModifiedBy>{}</cp:lastModifiedBy><cp:revision>1</cp:revision><dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{now}</dcterms:modified></cp:coreProperties>"#,
        xml_escape(&title),
        xml_escape(&subject),
        xml_escape(&author),
        xml_escape(&snapshot.id),
        xml_escape(&format!(
            "Transcribed from {} with SBWB; {} pages",
            snapshot.book.source_name, plan.stats.pages
        )),
        xml_escape(&author)
    );
    // docx-rs registers headers for each Section but not footers; add the
    // footer parts for every section except the last (document-level one).
    let section_pages: Vec<&crate::plan::PlannedPage> = plan
        .pages
        .iter()
        .filter(|p| p.break_after == PageBreak::Section)
        .collect();
    let mut extra_footers: Vec<(String, String)> = Vec::new(); // (part name, xml)
    if section_pages.len() > 1 {
        for (i, page) in section_pages[..section_pages.len() - 1].iter().enumerate() {
            if page.footer.is_empty() {
                continue;
            }
            let mut xml = String::from(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
            );
            for fp in &page.footer {
                xml.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="{}" /></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, style_for(fp), xml_escape(&fp.text())));
            }
            xml.push_str("</w:ftr>");
            extra_footers.push((format!("footerS{}.xml", i + 1), xml));
        }
    }
    let footer_for_section: Vec<Option<String>> = section_pages
        .iter()
        .enumerate()
        .map(|(i, page)| {
            if i + 1 < section_pages.len() && !page.footer.is_empty() {
                Some(format!("footerS{}.xml", i + 1))
            } else {
                None
            }
        })
        .collect();
    let mut input = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| SbwbError::other(format!("reopen docx: {e}")))?;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let re_sz = regex_lite_replace_all;
    for i in 0..input.len() {
        let mut f = input
            .by_index(i)
            .map_err(|e| SbwbError::other(format!("docx entry: {e}")))?;
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        let data = if name == "docProps/core.xml" {
            core.clone().into_bytes()
        } else if name == "word/document.xml" {
            let s = String::from_utf8_lossy(&data).into_owned();
            let s = re_sz(&s, "<w:pgSz ", "/>", &pg_sz);
            let s = re_sz(&s, "<w:pgMar ", "/>", &pg_mar);
            // every section break is a next-page break; sections get their footers
            let mut pieces = s.split("<w:sectPr>");
            let mut rebuilt = String::from(pieces.next().unwrap_or(""));
            for (i, rest) in pieces.enumerate() {
                rebuilt.push_str(r#"<w:sectPr><w:type w:val="nextPage" />"#);
                if let Some(Some(part)) = footer_for_section.get(i) {
                    rebuilt.push_str(&format!(
                        r#"<w:footerReference w:type="default" r:id="rId{}" />"#,
                        part.trim_end_matches(".xml")
                    ));
                }
                rebuilt.push_str(rest);
            }
            rebuilt.into_bytes()
        } else if name == "word/_rels/document.xml.rels" {
            let mut s = String::from_utf8_lossy(&data).into_owned();
            for (part, _) in &extra_footers {
                s = s.replace("</Relationships>", &format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="{part}" /></Relationships>"#, part.trim_end_matches(".xml")));
            }
            s.into_bytes()
        } else if name == "[Content_Types].xml" {
            let mut s = String::from_utf8_lossy(&data).into_owned();
            for (part, _) in &extra_footers {
                s = s.replace("</Types>", &format!(r#"<Override PartName="/word/{part}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml" /></Types>"#));
            }
            s.into_bytes()
        } else {
            data
        };
        out.start_file(name, opts)
            .map_err(|e| SbwbError::other(format!("zip: {e}")))?;
        out.write_all(&data)?;
    }
    for (part, xml) in &extra_footers {
        out.start_file(format!("word/{part}"), opts)
            .map_err(|e| SbwbError::other(format!("zip: {e}")))?;
        out.write_all(xml.as_bytes())?;
    }
    let cursor = out
        .finish()
        .map_err(|e| SbwbError::other(format!("zip finish: {e}")))?;
    Ok(cursor.into_inner())
}

/// Replace every `<prefix ... suffix>` element with `with`.
fn regex_lite_replace_all(s: &str, prefix: &str, suffix: &str, with: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find(prefix) {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        match after.find(suffix) {
            Some(end) => {
                out.push_str(with);
                rest = &after[end + suffix.len()..];
            }
            None => {
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}
