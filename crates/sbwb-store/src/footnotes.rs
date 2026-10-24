//! Footnote references (TXT-02, EXP-02): a link from a word in the text to
//! the footnote it calls. Numbers are not stored; they follow the reading
//! order of the linked words on each page (1, 2, 3, restarting per page).
//! The text pass suggests links where a printed marker (*, †, a digit) on a
//! word matches the marker a footnote starts with; the user accepts,
//! rejects, removes, or links by hand. A footnote can run to several
//! paragraphs: the user marks a footnote paragraph as continuing the note
//! above it. Links and continuations survive a text-pass rerun by finding
//! their words again.

use std::collections::{HashMap, HashSet};

use jiff::Timestamp;
use rusqlite::params;
use sbwb_core::{PageIndex, RegionId, Result, SbwbError, SpanId};
use sbwb_layout::RegionKind;
use sbwb_text::footnote::{note_marker, word_marker};
use sbwb_text::Span;
use serde::{Deserialize, Serialize};

use crate::project::{db, Project};

/// One reference from a word to a footnote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootnoteRef {
    pub id: String,
    pub page: u32,
    /// The word the reference follows.
    pub anchor: SpanId,
    /// The first word of the footnote paragraph.
    pub note: SpanId,
    /// The marker printed in the scan ("*", "†", "1"); stripped on export.
    pub marker: String,
    /// `linked`, `suggested`, or `rejected`.
    pub status: String,
    /// `manual` or `detected`.
    pub source: String,
    /// Position among the page's linked references, from 1.
    pub number: Option<u32>,
}

/// A footnote on a page: its first paragraph, and any paragraphs that
/// continue it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootnoteNote {
    /// First word of the paragraph; what a reference points to.
    pub span: SpanId,
    pub preview: String,
    /// The marker the note starts with, if any.
    pub marker: String,
    /// First words of the further paragraphs of this note, in order.
    #[serde(default)]
    pub parts: Vec<SpanId>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FootnotePage {
    pub notes: Vec<FootnoteNote>,
    /// Linked and suggested references (rejected ones are hidden).
    pub refs: Vec<FootnoteRef>,
}

/// Region kinds whose words can carry a reference.
fn anchor_kind(k: RegionKind) -> bool {
    matches!(
        k,
        RegionKind::Body
            | RegionKind::Heading
            | RegionKind::Marginalia
            | RegionKind::Table
            | RegionKind::Uncertain
    )
}

struct PageView {
    spans: Vec<Span>,
    /// Footnotes: (first span index, its marker). Paragraphs that continue
    /// a note are not listed here but in `parts`.
    notes: Vec<(usize, Option<String>)>,
    /// Every footnote paragraph's first span index, in order.
    starts: Vec<usize>,
    /// A note's first span index -> the first span indices of the
    /// paragraphs that continue it.
    parts: HashMap<usize, Vec<usize>>,
    /// Stored continuations: (row id, the paragraph start it resolves to).
    part_rows: Vec<(String, Option<usize>)>,
    /// Indices of words that may carry a reference.
    anchors: Vec<usize>,
}

impl PageView {
    fn note_preview(&self, i: usize) -> String {
        let mut out = String::new();
        let region = self.spans[i].region;
        for (k, s) in self.spans[i..].iter().enumerate() {
            if k > 0 && (s.paragraph_start || s.region != region) {
                break;
            }
            out.push_str(&s.text);
            out.push(' ');
            if out.chars().count() > 80 {
                break;
            }
        }
        let t = out.trim();
        if t.chars().count() > 80 {
            format!("{}…", t.chars().take(79).collect::<String>())
        } else {
            t.to_string()
        }
    }

    /// The note a footnote paragraph (by its first span index) belongs to.
    fn head_of(&self, start: usize) -> usize {
        self.parts
            .iter()
            .find(|(_, ps)| ps.contains(&start))
            .map(|(h, _)| *h)
            .unwrap_or(start)
    }

    /// The footnote paragraph a span is in: the nearest paragraph start at
    /// or before it with no region change in between.
    fn start_of(&self, i: usize) -> Option<usize> {
        self.starts
            .iter()
            .rev()
            .find(|&&st| st <= i)
            .filter(|&&st| {
                self.spans[st..=i]
                    .iter()
                    .all(|s| s.region == self.spans[i].region)
            })
            .copied()
    }
}

struct Row {
    id: String,
    anchor: String,
    note: String,
    marker: String,
    status: String,
    source: String,
    anchor_text: String,
    anchor_seq: i64,
    note_text: String,
    note_seq: i64,
}

fn now() -> String {
    Timestamp::now().to_string()
}

impl Project {
    fn footnote_view(&self, page: PageIndex) -> Result<PageView> {
        let spans = self.page_spans(page)?;
        let kinds: HashMap<RegionId, RegionKind> = self
            .page_layout(page)?
            .map(|l| {
                serde_json::from_value::<Vec<sbwb_layout::Region>>(l.regions).unwrap_or_default()
            })
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.id, r.kind))
            .collect();
        let kind = |s: &Span| s.region.and_then(|r| kinds.get(&r).copied());
        let mut all: Vec<(usize, Option<String>)> = Vec::new();
        let mut anchors = Vec::new();
        let mut prev_region: Option<Option<RegionId>> = None;
        for (i, s) in spans.iter().enumerate() {
            let starts = s.paragraph_start || prev_region != Some(s.region);
            prev_region = Some(s.region);
            match kind(s) {
                Some(RegionKind::Footnote) => {
                    if starts && !s.text.is_empty() {
                        all.push((i, note_marker(&s.text)));
                    } else if starts {
                        // an emptied first word: the paragraph starts at the next word
                        prev_region = None;
                    }
                }
                Some(k) if anchor_kind(k) && !s.text.is_empty() => anchors.push(i),
                _ => {}
            }
        }
        // Continuations: found by their span, or after a rerun by their word
        // nearest to where it was. The page's first footnote paragraph has
        // nothing above it to continue.
        let starts: Vec<usize> = all.iter().map(|(i, _)| *i).collect();
        let part_rows: Vec<(String, Option<usize>)> = self
            .footnote_part_rows(page)?
            .into_iter()
            .map(|(id, span, word, seq)| {
                let at = starts
                    .iter()
                    .copied()
                    .find(|&i| spans[i].id.to_string() == span)
                    .or_else(|| {
                        starts
                            .iter()
                            .copied()
                            .filter(|&i| spans[i].text == word)
                            .min_by_key(|&i| (spans[i].seq as i64 - seq).abs())
                    })
                    .filter(|&i| Some(&i) != starts.first());
                (id, at)
            })
            .collect();
        let continued: HashSet<usize> = part_rows.iter().filter_map(|(_, at)| *at).collect();
        let mut notes = Vec::new();
        let mut parts: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, marker) in all {
            match notes.last() {
                Some(&(head, _)) if continued.contains(&i) => {
                    parts.entry(head).or_default().push(i)
                }
                _ => notes.push((i, marker)),
            }
        }
        Ok(PageView {
            spans,
            notes,
            starts,
            parts,
            part_rows,
            anchors,
        })
    }

    fn footnote_part_rows(&self, page: PageIndex) -> Result<Vec<(String, String, String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, span, word, seq FROM footnote_parts WHERE page_index = ?1 ORDER BY created_at, rowid")
            .map_err(db)?;
        let rows = stmt
            .query_map(params![page.0 as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    /// Make a footnote paragraph (by its first word) part of the note above
    /// it (`on`), or a note of its own again. A paragraph that becomes part
    /// of another note loses any link that pointed to it as a note.
    pub fn footnote_continue(&mut self, span: SpanId, on: bool) -> Result<FootnotePage> {
        self.require_write()?;
        let page = crate::review::span_page(&self.conn, span)?;
        let view = self.footnote_view(page)?;
        let i = view
            .starts
            .iter()
            .copied()
            .find(|&i| view.spans[i].id == span)
            .ok_or_else(|| {
                SbwbError::InvalidInput("that is not the first word of a footnote paragraph".into())
            })?;
        if on && view.starts.first() == Some(&i) {
            return Err(SbwbError::InvalidInput(
                "the first footnote on the page has no footnote above it to continue".into(),
            ));
        }
        let s = &view.spans[i];
        let tx = self.conn.transaction().map_err(db)?;
        for (id, at) in &view.part_rows {
            if *at == Some(i) {
                tx.execute("DELETE FROM footnote_parts WHERE id = ?1", params![id])
                    .map_err(db)?;
            }
        }
        if on {
            tx.execute(
                "INSERT INTO footnote_parts(id, page_index, span, word, seq, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    sbwb_core::HistoryId::new().to_string(),
                    page.0 as i64,
                    s.id.to_string(),
                    s.text,
                    s.seq as i64,
                    now()
                ],
            )
            .map_err(db)?;
            tx.execute(
                "DELETE FROM footnote_refs WHERE page_index = ?1 AND note_span = ?2",
                params![page.0 as i64, s.id.to_string()],
            )
            .map_err(db)?;
        }
        bump(&tx, page)?;
        tx.commit().map_err(db)?;
        self.footnotes(page)
    }

    fn footnote_rows(&self, page: PageIndex) -> Result<Vec<Row>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, anchor_span, note_span, marker, status, source, anchor_text, anchor_seq, note_text, note_seq FROM footnote_refs WHERE page_index = ?1 ORDER BY created_at, rowid")
            .map_err(db)?;
        let rows = stmt
            .query_map(params![page.0 as i64], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    anchor: r.get(1)?,
                    note: r.get(2)?,
                    marker: r.get(3)?,
                    status: r.get(4)?,
                    source: r.get(5)?,
                    anchor_text: r.get(6)?,
                    anchor_seq: r.get(7)?,
                    note_text: r.get(8)?,
                    note_seq: r.get(9)?,
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    /// The page's footnotes and their references, numbered.
    pub fn footnotes(&self, page: PageIndex) -> Result<FootnotePage> {
        let view = self.footnote_view(page)?;
        let pos: HashMap<SpanId, usize> = view
            .spans
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();
        let mut refs: Vec<FootnoteRef> = self
            .footnote_rows(page)?
            .into_iter()
            .filter(|r| r.status != "rejected")
            .filter_map(|r| {
                let anchor = SpanId::parse(&r.anchor)?;
                let note = SpanId::parse(&r.note)?;
                (pos.contains_key(&anchor) && pos.contains_key(&note)).then_some(FootnoteRef {
                    id: r.id,
                    page: page.0,
                    anchor,
                    note,
                    marker: r.marker,
                    status: r.status,
                    source: r.source,
                    number: None,
                })
            })
            .collect();
        refs.sort_by_key(|r| pos[&r.anchor]);
        let mut n = 0;
        for r in refs.iter_mut().filter(|r| r.status == "linked") {
            n += 1;
            r.number = Some(n);
        }
        let notes = view
            .notes
            .iter()
            .map(|(i, m)| FootnoteNote {
                span: view.spans[*i].id,
                preview: view.note_preview(*i),
                marker: m.clone().unwrap_or_default(),
                parts: view
                    .parts
                    .get(i)
                    .into_iter()
                    .flatten()
                    .map(|&k| view.spans[k].id)
                    .collect(),
            })
            .collect();
        Ok(FootnotePage { notes, refs })
    }

    /// Link a word to a footnote paragraph (any word of it). Replaces other
    /// links of that word or that note.
    pub fn footnote_link(&mut self, anchor: SpanId, note: SpanId) -> Result<FootnotePage> {
        self.require_write()?;
        let page = crate::review::span_page(&self.conn, anchor)?;
        let view = self.footnote_view(page)?;
        let ai = view
            .spans
            .iter()
            .position(|s| s.id == anchor)
            .filter(|i| view.anchors.contains(i))
            .ok_or_else(|| {
                SbwbError::InvalidInput(
                    "a footnote reference goes on a word of the text, not on a heading line, footnote, or margin furniture".into(),
                )
            })?;
        // any word of a note, its continuing paragraphs included
        let ni = view.spans.iter().position(|s| s.id == note);
        let (ni, note_marker) = ni
            .and_then(|ni| view.start_of(ni))
            .map(|st| view.head_of(st))
            .and_then(|head| view.notes.iter().find(|(start, _)| *start == head).cloned())
            .ok_or_else(|| SbwbError::InvalidInput("that is not a footnote on this page".into()))?;
        let a = &view.spans[ai];
        let n = &view.spans[ni];
        let marker = match (word_marker(&a.text), note_marker) {
            (Some(w), Some(m)) if w == m => m,
            (Some(w), None) => w,
            _ => String::new(),
        };
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "DELETE FROM footnote_refs WHERE page_index = ?1 AND (anchor_span = ?2 OR note_span = ?3)",
            params![page.0 as i64, a.id.to_string(), n.id.to_string()],
        )
        .map_err(db)?;
        tx.execute(
            "INSERT INTO footnote_refs(id, page_index, anchor_span, note_span, marker, status, source, anchor_text, anchor_seq, note_text, note_seq, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'linked', 'manual', ?6, ?7, ?8, ?9, ?10)",
            params![
                sbwb_core::HistoryId::new().to_string(),
                page.0 as i64,
                a.id.to_string(),
                n.id.to_string(),
                marker,
                a.text,
                a.seq as i64,
                n.text,
                n.seq as i64,
                now()
            ],
        )
        .map_err(db)?;
        bump(&tx, page)?;
        tx.commit().map_err(db)?;
        self.footnotes(page)
    }

    /// Accept a suggestion, or reject / remove a reference. A removed
    /// detected link is remembered as rejected so it is not suggested again.
    pub fn footnote_decide(&mut self, id: &str, accept: bool) -> Result<FootnotePage> {
        self.require_write()?;
        let (page, source): (i64, String) = self
            .conn
            .query_row(
                "SELECT page_index, source FROM footnote_refs WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| SbwbError::NotFound("footnote reference not found".into()))?;
        let tx = self.conn.transaction().map_err(db)?;
        if accept {
            tx.execute(
                "UPDATE footnote_refs SET status = 'linked' WHERE id = ?1",
                params![id],
            )
            .map_err(db)?;
        } else if source == "detected" {
            tx.execute(
                "UPDATE footnote_refs SET status = 'rejected' WHERE id = ?1",
                params![id],
            )
            .map_err(db)?;
        } else {
            tx.execute("DELETE FROM footnote_refs WHERE id = ?1", params![id])
                .map_err(db)?;
        }
        let page = PageIndex(page as u32);
        bump(&tx, page)?;
        tx.commit().map_err(db)?;
        self.footnotes(page)
    }

    /// Accept every suggestion on a page.
    pub fn footnote_accept_all(&mut self, page: PageIndex) -> Result<FootnotePage> {
        self.require_write()?;
        let n = self
            .conn
            .execute(
                "UPDATE footnote_refs SET status = 'linked' WHERE page_index = ?1 AND status = 'suggested'",
                params![page.0 as i64],
            )
            .map_err(db)?;
        if n > 0 {
            bump(&self.conn, page)?;
        }
        self.footnotes(page)
    }

    /// Look for printed markers on a page now (the text pass does this on
    /// every run).
    pub fn footnote_detect(&mut self, page: PageIndex) -> Result<FootnotePage> {
        self.require_write()?;
        self.refresh_footnotes(page)?;
        self.footnotes(page)
    }

    /// Keep references attached after the page's words changed (a rerun
    /// replaces every word), then suggest links for printed markers.
    pub(crate) fn refresh_footnotes(&mut self, page: PageIndex) -> Result<()> {
        let view = self.footnote_view(page)?;
        if view.notes.is_empty() {
            self.conn
                .execute(
                    "DELETE FROM footnote_refs WHERE page_index = ?1 AND status = 'suggested'",
                    params![page.0 as i64],
                )
                .map_err(db)?;
            return Ok(());
        }
        let rows = self.footnote_rows(page)?;
        let by_id: HashMap<String, usize> = view
            .spans
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.to_string(), i))
            .collect();
        let note_starts: HashSet<usize> = view.notes.iter().map(|(i, _)| *i).collect();
        let nearest = |cands: &mut dyn Iterator<Item = usize>, text: &str, seq: i64| {
            cands
                .filter(|&i| view.spans[i].text == text)
                .min_by_key(|&i| (view.spans[i].seq as i64 - seq).abs())
        };
        let tx = self.conn.transaction().map_err(db)?;
        for (id, at) in &view.part_rows {
            match at {
                Some(i) => {
                    let s = &view.spans[*i];
                    tx.execute(
                        "UPDATE footnote_parts SET span = ?2, word = ?3, seq = ?4 WHERE id = ?1",
                        params![id, s.id.to_string(), s.text, s.seq as i64],
                    )
                    .map_err(db)?;
                }
                None => {
                    tx.execute("DELETE FROM footnote_parts WHERE id = ?1", params![id])
                        .map_err(db)?;
                }
            }
        }
        tx.execute(
            "DELETE FROM footnote_refs WHERE page_index = ?1 AND status = 'suggested'",
            params![page.0 as i64],
        )
        .map_err(db)?;
        let mut used_anchor: HashSet<usize> = HashSet::new();
        let mut used_note: HashSet<usize> = HashSet::new();
        let mut rejected: HashSet<(usize, usize)> = HashSet::new();
        for r in rows.iter().filter(|r| r.status != "suggested") {
            let a = by_id
                .get(&r.anchor)
                .copied()
                .filter(|i| view.anchors.contains(i))
                .or_else(|| {
                    nearest(
                        &mut view.anchors.iter().copied(),
                        &r.anchor_text,
                        r.anchor_seq,
                    )
                });
            let n = by_id
                .get(&r.note)
                .copied()
                .filter(|i| note_starts.contains(i))
                .or_else(|| {
                    nearest(
                        &mut view.notes.iter().map(|(i, _)| *i),
                        &r.note_text,
                        r.note_seq,
                    )
                });
            match (a, n) {
                (Some(a), Some(n)) => {
                    let (sa, sn) = (&view.spans[a], &view.spans[n]);
                    tx.execute(
                        "UPDATE footnote_refs SET anchor_span = ?2, note_span = ?3, anchor_text = ?4, anchor_seq = ?5, note_text = ?6, note_seq = ?7 WHERE id = ?1",
                        params![r.id, sa.id.to_string(), sn.id.to_string(), sa.text, sa.seq as i64, sn.text, sn.seq as i64],
                    )
                    .map_err(db)?;
                    if r.status == "linked" {
                        used_anchor.insert(a);
                        used_note.insert(n);
                    } else {
                        rejected.insert((a, n));
                    }
                }
                _ => {
                    tx.execute("DELETE FROM footnote_refs WHERE id = ?1", params![r.id])
                        .map_err(db)?;
                }
            }
        }
        // Suggestions: each marked note takes the first free word in reading
        // order that ends with the same marker.
        for (ni, marker) in &view.notes {
            let Some(marker) = marker else { continue };
            if used_note.contains(ni) {
                continue;
            }
            let Some(&ai) = view.anchors.iter().find(|&&ai| {
                !used_anchor.contains(&ai)
                    && !rejected.contains(&(ai, *ni))
                    && word_marker(&view.spans[ai].text).as_deref() == Some(marker.as_str())
            }) else {
                continue;
            };
            used_anchor.insert(ai);
            used_note.insert(*ni);
            let (a, n) = (&view.spans[ai], &view.spans[*ni]);
            tx.execute(
                "INSERT INTO footnote_refs(id, page_index, anchor_span, note_span, marker, status, source, anchor_text, anchor_seq, note_text, note_seq, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'suggested', 'detected', ?6, ?7, ?8, ?9, ?10)",
                params![
                    sbwb_core::HistoryId::new().to_string(),
                    page.0 as i64,
                    a.id.to_string(),
                    n.id.to_string(),
                    marker,
                    a.text,
                    a.seq as i64,
                    n.text,
                    n.seq as i64,
                    now()
                ],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)
    }
}

/// A reference change is a text change for approval purposes.
fn bump(conn: &rusqlite::Connection, page: PageIndex) -> Result<()> {
    conn.execute(
        "UPDATE pages SET text_revision = text_revision + 1 WHERE page_index = ?1",
        params![page.0 as i64],
    )
    .map_err(db)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::{PageScope, Rect};
    use sbwb_layout::Region;
    use sbwb_text::SpanOrigin;

    fn project(dir: &std::path::Path) -> Project {
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
            &dir.join("b.sbwb"),
            &src,
            info,
            &[],
            PageScope::all(1),
            "test",
        )
        .unwrap();
        p.set_stage_done(PageIndex(0), sbwb_core::Stage::TextPass, true)
            .unwrap();
        p
    }

    /// Body "The world* was made, India† too." and two notes "* First." and
    /// "† Second.".
    fn page_spans(body: RegionId, foot: RegionId) -> Vec<Span> {
        let words = [
            (body, "The", true),
            (body, "world*", false),
            (body, "was", false),
            (body, "made,", false),
            (body, "India†", false),
            (body, "too.", false),
            (foot, "*", true),
            (foot, "First.", false),
            (foot, "†", true),
            (foot, "Second.", false),
        ];
        words
            .iter()
            .enumerate()
            .map(|(i, (r, t, start))| Span {
                id: SpanId::new(),
                page: PageIndex(0),
                seq: i as u32,
                region: Some(*r),
                text: t.to_string(),
                anchors: vec![],
                origin: SpanOrigin::Ocr,
                confidence: Some(95.0),
                trailing: " ".into(),
                revision: 1,
                protected: false,
                structure: Default::default(),
                paragraph_start: *start,
            })
            .collect()
    }

    #[test]
    fn suggest_accept_number_reject_and_rerun() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = project(dir.path());
        let page = PageIndex(0);
        let body = Region::new(RegionKind::Body, Rect::new(50.0, 50.0, 400.0, 500.0), 0);
        let foot = Region::new(RegionKind::Footnote, Rect::new(50.0, 600.0, 400.0, 80.0), 1);
        let regions = serde_json::to_value(vec![body.clone(), foot.clone()]).unwrap();
        p.put_page_layout(
            page,
            None,
            &regions,
            &serde_json::json!({}),
            None,
            true,
            true,
        )
        .unwrap();
        p.put_page_text(page, None, &page_spans(body.id, foot.id), &[])
            .unwrap();
        let f = p.footnotes(page).unwrap();
        assert_eq!(f.notes.len(), 2);
        assert_eq!(f.notes[0].marker, "*");
        assert_eq!(f.refs.len(), 2, "both markers suggested");
        assert!(f
            .refs
            .iter()
            .all(|r| r.status == "suggested" && r.number.is_none()));
        // accept the second only: it is number 1
        let f = p.footnote_decide(&f.refs[1].id, true).unwrap();
        let linked: Vec<_> = f.refs.iter().filter(|r| r.status == "linked").collect();
        assert_eq!((linked.len(), linked[0].number), (1, Some(1)));
        // accept all: numbers follow reading order
        let f = p.footnote_accept_all(page).unwrap();
        assert_eq!(
            f.refs.iter().map(|r| r.number).collect::<Vec<_>>(),
            [Some(1), Some(2)]
        );
        // remove a detected link: it is not suggested again
        let first = f.refs[0].clone();
        let f = p.footnote_decide(&first.id, false).unwrap();
        assert_eq!(f.refs.len(), 1);
        assert_eq!(f.refs[0].number, Some(1));
        p.rebuild_issues(page).unwrap();
        assert_eq!(p.footnotes(page).unwrap().refs.len(), 1);
        // a manual link on another word replaces nothing else
        let spans = p.page_spans(page).unwrap();
        let f = p.footnote_link(spans[3].id, spans[7].id).unwrap();
        let manual = f.refs.iter().find(|r| r.source == "manual").unwrap();
        assert_eq!(
            manual.note, spans[6].id,
            "any word of a note links its first word"
        );
        assert_eq!(manual.marker, "", "made, carries no marker");
        assert_eq!(
            f.refs.iter().map(|r| r.number).collect::<Vec<_>>(),
            [Some(1), Some(2)]
        );
        // footnote words and furniture cannot carry a reference
        assert!(p.footnote_link(spans[7].id, spans[6].id).is_err());
        // rerun: new span ids; links follow their words
        p.put_page_text(page, None, &page_spans(body.id, foot.id), &[])
            .unwrap();
        let f = p.footnotes(page).unwrap();
        let spans = p.page_spans(page).unwrap();
        assert_eq!(f.refs.len(), 2);
        assert_eq!(f.refs[0].anchor, spans[3].id);
        assert_eq!(f.refs[1].anchor, spans[4].id);
        assert!(f.refs.iter().all(|r| r.status == "linked"));
    }

    #[test]
    fn a_footnote_can_run_to_several_paragraphs() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = project(dir.path());
        let page = PageIndex(0);
        let body = Region::new(RegionKind::Body, Rect::new(50.0, 50.0, 400.0, 500.0), 0);
        let foot = Region::new(RegionKind::Footnote, Rect::new(50.0, 600.0, 400.0, 80.0), 1);
        // the second paragraph of the first note sits in a box of its own
        let foot2 = Region::new(RegionKind::Footnote, Rect::new(50.0, 690.0, 400.0, 40.0), 2);
        let regions =
            serde_json::to_value(vec![body.clone(), foot.clone(), foot2.clone()]).unwrap();
        p.put_page_layout(
            page,
            None,
            &regions,
            &serde_json::json!({}),
            None,
            true,
            true,
        )
        .unwrap();
        // "* First." / "More of it." (another box) / "† Second."
        let mut spans = page_spans(body.id, foot.id);
        let more: Vec<Span> = ["More", "of", "it."]
            .iter()
            .enumerate()
            .map(|(k, t)| Span {
                id: SpanId::new(),
                seq: 0,
                region: Some(foot2.id),
                text: t.to_string(),
                paragraph_start: k == 0,
                ..spans[7].clone()
            })
            .collect();
        spans.splice(8..8, more);
        for (k, s) in spans.iter_mut().enumerate() {
            s.seq = k as u32;
        }
        p.put_page_text(page, None, &spans, &[]).unwrap();
        let f = p.footnotes(page).unwrap();
        assert_eq!(f.notes.len(), 3, "each paragraph is a note to begin with");
        let s = p.page_spans(page).unwrap();
        let more = s[8].id;
        // the first note can not continue anything
        assert!(p.footnote_continue(s[6].id, true).is_err());
        // a link to the paragraph as a note goes when it becomes a part
        p.footnote_link(s[3].id, more).unwrap();
        let f = p.footnote_continue(more, true).unwrap();
        assert_eq!(f.notes.len(), 2);
        assert_eq!(f.notes[0].parts, [more]);
        assert!(f.refs.iter().all(|r| r.note != more));
        // linking a word of the continuation links the whole note
        let f = p.footnote_link(s[3].id, s[9].id).unwrap();
        let r = f.refs.iter().find(|r| r.anchor == s[3].id).unwrap();
        assert_eq!(r.note, s[6].id);
        // a rerun keeps it
        p.put_page_text(page, None, &spans_again(&spans), &[])
            .unwrap();
        let f = p.footnotes(page).unwrap();
        let s2 = p.page_spans(page).unwrap();
        assert_eq!(f.notes.len(), 2);
        assert_eq!(f.notes[0].parts, [s2[8].id]);
        // and it can be a note of its own again
        let f = p.footnote_continue(s2[8].id, false).unwrap();
        assert_eq!(f.notes.len(), 3);
        assert!(f.notes.iter().all(|n| n.parts.is_empty()));
    }

    /// The same words with new ids, as a text-pass rerun makes them.
    fn spans_again(spans: &[Span]) -> Vec<Span> {
        spans
            .iter()
            .map(|s| Span {
                id: SpanId::new(),
                ..s.clone()
            })
            .collect()
    }
}
