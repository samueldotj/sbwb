//! Targeted refinement (OCR-02, M8.4): merge region-OCR readings into the
//! existing text as candidates. Nothing is applied automatically; human
//! edited or approved text is never replaced. Words with no existing span
//! become new spans (a drawn region can recover text initial OCR missed).

use jiff::Timestamp;
use rusqlite::{params, OptionalExtension};
use sbwb_core::{PageIndex, ProposalId, Rect, RegionId, Result, RunId, SpanId};
use sbwb_ocr::{OcrWord, SecondOutput};
use sbwb_text::{Anchor, Span, SpanOrigin, WordRef};
use serde::{Deserialize, Serialize};

use crate::project::{db, Project};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MergeOutcome {
    pub words: u32,
    pub proposals_added: u32,
    pub spans_inserted: u32,
    pub agreements: u32,
    pub second_engine_disagreements: u32,
}

fn core(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
}

fn best_overlap<'a>(spans: &'a [Span], page: PageIndex, bbox: &Rect) -> Option<(&'a Span, f64)> {
    let mut best: Option<(&Span, f64)> = None;
    for s in spans {
        for a in s.anchors.iter().filter(|a| a.page == page) {
            let f = bbox
                .overlap_fraction(&a.bbox)
                .max(a.bbox.overlap_fraction(bbox));
            if f > best.map(|b| b.1).unwrap_or(0.0) {
                best = Some((s, f));
            }
        }
    }
    best.filter(|b| b.1 >= 0.4)
}

impl Project {
    /// Merge region-OCR words (page points) and optional second-engine
    /// lines into the page. Returns what changed; the issue index is
    /// rebuilt so new evidence appears in the inbox.
    pub fn merge_region_candidates(
        &mut self,
        page: PageIndex,
        run: RunId,
        region: Option<RegionId>,
        words: &[OcrWord],
        second: Option<&SecondOutput>,
        source_label: &str,
    ) -> Result<MergeOutcome> {
        self.require_write()?;
        let mut spans = self.page_spans(page)?;
        let mut out = MergeOutcome::default();
        let now = Timestamp::now().to_string();
        let mut proposals: Vec<(SpanId, u64, String, String, u8, String, String)> = Vec::new(); // span, rev, original, replacement, score, reason, source
        let mut inserted: Vec<Span> = Vec::new();
        for (i, w) in words.iter().enumerate() {
            let text = w.text.trim();
            if text.is_empty() {
                continue;
            }
            out.words += 1;
            match best_overlap(&spans, page, &w.bbox) {
                Some((s, _)) => {
                    if core(&s.text) == core(text) {
                        out.agreements += 1;
                    } else {
                        proposals.push((
                            s.id,
                            s.revision,
                            s.text.clone(),
                            text.to_string(),
                            w.confidence.round().clamp(0.0, 100.0) as u8,
                            format!(
                                "{source_label} read “{text}” ({:.0}% confidence)",
                                w.confidence
                            ),
                            "region_ocr".into(),
                        ));
                    }
                }
                None => {
                    inserted.push(Span {
                        id: SpanId::new(),
                        page,
                        seq: 0,
                        region,
                        text: text.to_string(),
                        anchors: vec![Anchor {
                            page,
                            region,
                            words: vec![WordRef {
                                run,
                                index: i as u32,
                            }],
                            bbox: w.bbox,
                            approximate: false,
                        }],
                        origin: SpanOrigin::Ocr,
                        confidence: Some(w.confidence),
                        trailing: " ".into(),
                        revision: 1,
                        protected: false,
                        structure: Default::default(),
                        paragraph_start: false,
                    });
                }
            }
        }
        // Second engine: compare each line against the spans it covers, word by word.
        if let Some(sec) = second {
            for line in &sec.lines {
                for (text, bbox) in &line.words {
                    if let Some((s, _)) = best_overlap(&spans, page, bbox) {
                        if core(&s.text) != core(text) && !text.trim().is_empty() {
                            out.second_engine_disagreements += 1;
                            proposals.push((
                                s.id,
                                s.revision,
                                s.text.clone(),
                                text.trim().to_string(),
                                0,
                                format!(
                                    "[{} · line-level] read “{}” in the line “{}” (no word score)",
                                    sec.engine,
                                    text.trim(),
                                    line.text.trim()
                                ),
                                "second_engine".into(),
                            ));
                        }
                    }
                }
            }
        }
        // Insert new spans in reading order by anchor position (top-down, then left-right).
        if !inserted.is_empty() {
            inserted.sort_by(|a, b| {
                let (ra, rb) = (&a.anchors[0].bbox, &b.anchors[0].bbox);
                let same_line = (ra.y - rb.y).abs() < ra.h.min(rb.h) * 0.6;
                if same_line {
                    ra.x.partial_cmp(&rb.x).unwrap()
                } else {
                    ra.y.partial_cmp(&rb.y).unwrap()
                }
            });
            for mut s in inserted {
                let cy = s.anchors[0].bbox.y + s.anchors[0].bbox.h / 2.0;
                let cx = s.anchors[0].bbox.x;
                // after the last span (same region when known) whose anchor is above, or on the same line to the left
                let candidates: Vec<usize> = spans
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| region.is_none() || e.region == region)
                    .filter(|(_, e)| {
                        e.anchors.iter().any(|a| {
                            a.page == page && {
                                let acy = a.bbox.y + a.bbox.h / 2.0;
                                acy + a.bbox.h * 0.4 < cy
                                    || ((acy - cy).abs() < a.bbox.h * 0.6 && a.bbox.x < cx)
                            }
                        })
                    })
                    .map(|(i, _)| i)
                    .collect();
                let at = candidates.last().map(|i| i + 1).unwrap_or_else(|| {
                    // no span above: before the first span of the region, else start
                    spans
                        .iter()
                        .position(|e| region.is_some() && e.region == region)
                        .unwrap_or(0)
                });
                if at == 0 && spans.first().map(|f| f.region == region).unwrap_or(false) {
                    s.paragraph_start = true;
                    if let Some(f) = spans.first_mut() {
                        f.paragraph_start = false;
                    }
                }
                if at >= spans.len() {
                    s.paragraph_start = spans.is_empty();
                }
                spans.insert(at.min(spans.len()), s);
                out.spans_inserted += 1;
            }
            for (i, s) in spans.iter_mut().enumerate() {
                s.seq = i as u32;
            }
        }
        let tx = self.conn.transaction().map_err(db)?;
        if out.spans_inserted > 0 {
            // rewrite the page's spans keeping ids (proposals and issues stay valid)
            tx.execute(
                "DELETE FROM spans WHERE page_index = ?1",
                params![page.0 as i64],
            )
            .map_err(db)?;
            let mut ins = tx
                .prepare(
                    "INSERT INTO spans(id, page_index, seq, region_id, text, trailing, origin, confidence, anchors, revision, protected, structure, paragraph_start)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                )
                .map_err(db)?;
            for s in &spans {
                ins.execute(params![
                    s.id.to_string(),
                    s.page.0 as i64,
                    s.seq as i64,
                    s.region.map(|r| r.to_string()),
                    s.text,
                    s.trailing,
                    serde_json::to_value(s.origin)?
                        .as_str()
                        .unwrap_or("ocr")
                        .to_string(),
                    s.confidence.map(|c| c as f64),
                    serde_json::to_string(&s.anchors)?,
                    s.revision as i64,
                    s.protected as i64,
                    serde_json::to_value(s.structure)?
                        .as_str()
                        .unwrap_or("text")
                        .to_string(),
                    s.paragraph_start as i64,
                ])
                .map_err(db)?;
            }
            drop(ins);
        }
        {
            let mut insp = tx
                .prepare(
                    "INSERT INTO proposals(id, page_index, span_id, span_revision, kind, original, replacement, score, reason, source, status, merged_span, cross_page, created_at, run_id)
                     VALUES (?1, ?2, ?3, ?4, 'spelling', ?5, ?6, ?7, ?8, ?9, 'open', NULL, 0, ?10, ?11)",
                )
                .map_err(db)?;
            for (span, rev, original, replacement, score, reason, source) in &proposals {
                // one open proposal per span and replacement
                let dup: i64 = tx
                    .query_row(
                        "SELECT count(*) FROM proposals WHERE span_id = ?1 AND replacement = ?2 AND status IN ('open', 'deferred')",
                        params![span.to_string(), replacement],
                        |r| r.get(0),
                    )
                    .map_err(db)?;
                if dup > 0 {
                    continue;
                }
                insp.execute(params![
                    ProposalId::new().to_string(),
                    page.0 as i64,
                    span.to_string(),
                    *rev as i64,
                    original,
                    replacement,
                    *score as i64,
                    reason,
                    source,
                    now,
                    run.to_string()
                ])
                .map_err(db)?;
                out.proposals_added += 1;
            }
        }
        tx.execute(
            "UPDATE pages SET text_revision = text_revision + 1 WHERE page_index = ?1",
            params![page.0 as i64],
        )
        .map_err(db)?;
        tx.commit().map_err(db)?;
        self.rebuild_issues(page)?;
        Ok(out)
    }
}

/// AI run record (AI-02 run history).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRunRecord {
    pub id: String,
    pub ts: String,
    pub provider: String,
    pub model: String,
    pub pages: Vec<u32>,
    pub consent: serde_json::Value,
    pub requests: u32,
    pub chars_sent: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub suggestions: u32,
    pub rejected: u32,
    pub status: String,
    pub error: Option<String>,
    pub elapsed_ms: Option<u64>,
}

impl Project {
    /// Add validated AI suggestions as open proposals labelled by provider
    /// and model (AI-03). Never applied; stale spans are skipped.
    pub fn merge_ai_suggestions(
        &mut self,
        page: PageIndex,
        label: &str,
        items: &[(SpanId, u64, String, String, String)],
    ) -> Result<u32> {
        // (span, revision, before, after, reason)
        self.require_write()?;
        let now = Timestamp::now().to_string();
        let mut added = 0;
        let tx = self.conn.transaction().map_err(db)?;
        for (span, revision, before, after, reason) in items {
            let current: Option<(String, i64)> = tx
                .query_row(
                    "SELECT text, revision FROM spans WHERE id = ?1",
                    params![span.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?;
            let Some((text, rev)) = current else { continue };
            if rev as u64 != *revision || &text != before {
                continue; // stale: the word changed since it was sent
            }
            let dup: i64 = tx
                .query_row(
                    "SELECT count(*) FROM proposals WHERE span_id = ?1 AND replacement = ?2 AND status IN ('open', 'deferred')",
                    params![span.to_string(), after],
                    |r| r.get(0),
                )
                .map_err(db)?;
            if dup > 0 {
                continue;
            }
            tx.execute(
                "INSERT INTO proposals(id, page_index, span_id, span_revision, kind, original, replacement, score, reason, source, status, merged_span, cross_page, created_at, run_id)
                 VALUES (?1, ?2, ?3, ?4, 'spelling', ?5, ?6, 0, ?7, ?8, 'open', NULL, 0, ?9, NULL)",
                params![ProposalId::new().to_string(), page.0 as i64, span.to_string(), *revision as i64, before, after, format!("[AI · {label}] {reason}"), format!("ai:{label}"), now],
            )
            .map_err(db)?;
            added += 1;
        }
        if added > 0 {
            tx.execute(
                "UPDATE pages SET text_revision = text_revision + 1 WHERE page_index = ?1",
                params![page.0 as i64],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)?;
        if added > 0 {
            self.rebuild_issues(page)?;
        }
        Ok(added)
    }

    pub fn record_ai_run(&self, r: &AiRunRecord) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "INSERT INTO ai_runs(id, ts, provider, model, pages, consent, requests, chars_sent, input_tokens, output_tokens, suggestions, rejected, status, error, elapsed_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT(id) DO UPDATE SET requests = excluded.requests, chars_sent = excluded.chars_sent, input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens, suggestions = excluded.suggestions, rejected = excluded.rejected, status = excluded.status, error = excluded.error, elapsed_ms = excluded.elapsed_ms",
                params![
                    r.id,
                    r.ts,
                    r.provider,
                    r.model,
                    serde_json::to_string(&r.pages)?,
                    serde_json::to_string(&r.consent)?,
                    r.requests as i64,
                    r.chars_sent as i64,
                    r.input_tokens.map(|t| t as i64),
                    r.output_tokens.map(|t| t as i64),
                    r.suggestions as i64,
                    r.rejected as i64,
                    r.status,
                    r.error,
                    r.elapsed_ms.map(|t| t as i64),
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn ai_runs(&self) -> Result<Vec<AiRunRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, ts, provider, model, pages, consent, requests, chars_sent, input_tokens, output_tokens, suggestions, rejected, status, error, elapsed_ms FROM ai_runs ORDER BY ts DESC LIMIT 50")
            .map_err(db)?;
        let rows = stmt
            .query_map([], |r| {
                let pages: String = r.get(4)?;
                let consent: String = r.get(5)?;
                Ok(AiRunRecord {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    provider: r.get(2)?,
                    model: r.get(3)?,
                    pages: serde_json::from_str(&pages).unwrap_or_default(),
                    consent: serde_json::from_str(&consent).unwrap_or_default(),
                    requests: r.get::<_, i64>(6)? as u32,
                    chars_sent: r.get::<_, i64>(7)? as u64,
                    input_tokens: r.get::<_, Option<i64>>(8)?.map(|t| t as u64),
                    output_tokens: r.get::<_, Option<i64>>(9)?.map(|t| t as u64),
                    suggestions: r.get::<_, i64>(10)? as u32,
                    rejected: r.get::<_, i64>(11)? as u32,
                    status: r.get(12)?,
                    error: r.get(13)?,
                    elapsed_ms: r.get::<_, Option<i64>>(14)?.map(|t| t as u64),
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    /// Open AI proposals across the book (for "Review N AI suggestions").
    pub fn ai_open_suggestions(&self) -> Result<u32> {
        self.conn
            .query_row(
                "SELECT count(*) FROM proposals WHERE source LIKE 'ai:%' AND status = 'open'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u32)
            .map_err(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::PageScope;
    use std::path::Path;

    fn create(dir: &Path) -> Project {
        let src = dir.join("book.pdf");
        let bytes = b"%PDF-1.4 fake".to_vec();
        std::fs::write(&src, &bytes).unwrap();
        let info = crate::project::SourceInfo {
            name: "book.pdf".into(),
            size: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            page_count: 1,
            title: None,
            author: None,
        };
        let p = Project::create(
            &dir.join("book.sbwb"),
            &src,
            info,
            &[(300.0, 500.0)],
            PageScope::all(1),
            "test",
        )
        .unwrap();
        p.set_page_status(PageIndex(0), sbwb_core::PageStatus::Done, None)
            .unwrap();
        p.set_stage_done(PageIndex(0), sbwb_core::Stage::TextPass, true)
            .unwrap();
        p
    }

    fn span(seq: u32, text: &str, x: f64, y: f64, origin: SpanOrigin) -> Span {
        Span {
            id: SpanId::new(),
            page: PageIndex(0),
            seq,
            region: None,
            text: text.into(),
            anchors: vec![Anchor {
                page: PageIndex(0),
                region: None,
                words: vec![],
                bbox: Rect::new(x, y, 30.0, 10.0),
                approximate: false,
            }],
            origin,
            confidence: Some(80.0),
            trailing: " ".into(),
            revision: 1,
            protected: false,
            structure: Default::default(),
            paragraph_start: seq == 0,
        }
    }

    fn word(text: &str, x: f64, y: f64, conf: f32) -> OcrWord {
        OcrWord {
            block: 0,
            paragraph: 0,
            line: 0,
            index: 0,
            bbox: Rect::new(x, y, 30.0, 10.0),
            bbox_px: Rect::new(0.0, 0.0, 1.0, 1.0),
            confidence: conf,
            text: text.into(),
        }
    }

    #[test]
    fn candidates_are_added_and_missing_words_inserted_without_replacing_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = create(dir.path());
        let mut edited = span(1, "hand", 40.0, 10.0, SpanOrigin::Manual);
        edited.confidence = None;
        let spans = vec![
            span(0, "The", 5.0, 10.0, SpanOrigin::Ocr),
            edited,
            span(2, "of", 5.0, 40.0, SpanOrigin::Ocr),
        ];
        p.put_page_text(PageIndex(0), None, &spans, &[]).unwrap();
        let run = RunId::new();
        let words = vec![
            word("The", 5.0, 10.0, 95.0),
            word("band", 40.0, 10.0, 88.0),
            word("MARGIN", 5.0, 25.0, 70.0),
            word("of", 5.0, 40.0, 90.0),
        ];
        let out = p
            .merge_region_candidates(
                PageIndex(0),
                run,
                None,
                &words,
                None,
                "region OCR 300 dpi ×3",
            )
            .unwrap();
        assert_eq!(
            (
                out.words,
                out.agreements,
                out.proposals_added,
                out.spans_inserted
            ),
            (4, 2, 1, 1)
        );
        let s = p.page_spans(PageIndex(0)).unwrap();
        let texts: Vec<&str> = s.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["The", "hand", "MARGIN", "of"],
            "inserted in reading order; the manual edit is untouched"
        );
        assert_eq!(s[1].origin, SpanOrigin::Manual);
        let props = p.page_proposals(PageIndex(0)).unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(
            (
                props[0].original.as_str(),
                props[0].replacement.as_str(),
                props[0].status.as_str()
            ),
            ("hand", "band", "open")
        );
        let issues = p.issues_for_page(PageIndex(0)).unwrap();
        assert!(issues
            .iter()
            .any(|i| i.original == "hand" && i.replacement.as_deref() == Some("band")));
    }
}
