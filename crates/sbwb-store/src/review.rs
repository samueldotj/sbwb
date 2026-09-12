//! Review persistence (REV-01..06, PRJ-04, PRJ-05): the issue index,
//! decisions anchored to span revisions, approvals, grouped corrections,
//! guarded undo, and edit drafts.

use jiff::Timestamp;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sbwb_core::{HistoryId, IssueId, PageIndex, PageScope, ProposalId, Result, SbwbError, SpanId};
use sbwb_review::{
    build_issues, decision_key, group_matches_on_page, Candidate, GroupMatch, Issue, IssueCounts,
    IssueFilter, IssueKind, IssueStatus,
};
use sbwb_text::SpanOrigin;
use serde::{Deserialize, Serialize};

use crate::project::{db, Project};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decision {
    /// Take a candidate reading (or any text the user typed).
    Accept { text: String },
    /// Free-text edit.
    Edit { text: String },
    /// Keep the original; resolves this exact proposal.
    Skip,
    /// Defer.
    Later,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOutcome {
    pub issue: Issue,
    pub history: HistoryId,
    pub span_text: String,
    pub span_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextIssue {
    pub issue: Option<Issue>,
    pub wrapped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageIssueCounts {
    pub page: u32,
    pub open: u32,
    pub matching: u32,
    pub deferred: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupOutcome {
    pub applied: u32,
    pub history: Option<HistoryId>,
    /// Spans whose revision moved since the preview: nothing was applied.
    pub stale: Vec<SpanId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: HistoryId,
    pub ts: String,
    pub kind: String,
    pub label: String,
    pub undone: bool,
    /// Whether the entry carries an undo payload.
    pub undoable: bool,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub span: SpanId,
    pub page: u32,
    pub text: String,
    pub updated_at: String,
}

fn now() -> String {
    Timestamp::now().to_string()
}

fn read_issue(r: &rusqlite::Row) -> rusqlite::Result<Issue> {
    let id: String = r.get(0)?;
    let span: String = r.get(3)?;
    let kind: String = r.get(5)?;
    let proposal: Option<String> = r.get(9)?;
    let status: String = r.get(13)?;
    let candidates: String = r.get(16)?;
    let bbox: Option<String> = r.get(17)?;
    let region: Option<String> = r.get(18)?;
    Ok(Issue {
        id: IssueId::parse(&id).unwrap_or_default(),
        page: PageIndex(r.get::<_, i64>(1)? as u32),
        seq: r.get::<_, i64>(2)? as u32,
        span: SpanId::parse(&span).unwrap_or_default(),
        span_revision: r.get::<_, i64>(4)? as u64,
        kind: IssueKind::parse(&kind).unwrap_or(IssueKind::ConflictingReadings),
        score: r.get::<_, Option<i64>>(6)?.map(|s| s as u8),
        score_source: r.get(7)?,
        priority: r.get::<_, i64>(8)? as u8,
        proposal: proposal.and_then(|p| ProposalId::parse(&p)),
        original: r.get(10)?,
        replacement: r.get(11)?,
        reason: r.get(12)?,
        status: IssueStatus::parse(&status),
        decision: r.get(14)?,
        note: r.get(15)?,
        candidates: serde_json::from_str(&candidates).unwrap_or_default(),
        bbox: bbox.and_then(|b| serde_json::from_str(&b).ok()),
        region: region.and_then(|r| sbwb_core::RegionId::parse(&r)),
    })
}

const ISSUE_COLS: &str = "id, page_index, seq, span_id, span_revision, kind, score, score_source, priority, proposal_id, original, replacement, reason, status, decision, note, candidates, bbox, region_id";

fn insert_issue(tx: &Connection, i: &Issue) -> Result<()> {
    tx.execute(
        &format!("INSERT INTO issues({ISSUE_COLS}, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)"),
        params![
            i.id.to_string(),
            i.page.0 as i64,
            i.seq as i64,
            i.span.to_string(),
            i.span_revision as i64,
            i.kind.as_str(),
            i.score.map(|s| s as i64),
            i.score_source,
            i.priority as i64,
            i.proposal.map(|p| p.to_string()),
            i.original,
            i.replacement,
            i.reason,
            i.status.as_str(),
            i.decision,
            i.note,
            serde_json::to_string(&i.candidates)?,
            i.bbox.map(|b| serde_json::to_string(&b).unwrap_or_default()),
            i.region.map(|r| r.to_string()),
            now(),
        ],
    )
    .map_err(db)?;
    Ok(())
}

/// Effective-text change inside a transaction. Edits never inherit
/// confidence (PROV-01). Returns the new revision.
fn update_span_text(
    tx: &Transaction,
    span: SpanId,
    text: &str,
    origin: SpanOrigin,
    expected_revision: u64,
) -> Result<u64> {
    let current: Option<i64> = tx
        .query_row(
            "SELECT revision FROM spans WHERE id = ?1",
            params![span.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(db)?;
    let Some(current) = current else {
        return Err(SbwbError::NotFound("the word no longer exists".into()));
    };
    if current as u64 != expected_revision {
        return Err(SbwbError::Conflict(
            "this word changed since it was loaded; reload the page".into(),
        ));
    }
    let next = expected_revision + 1;
    let origin = serde_json::to_value(origin)?
        .as_str()
        .unwrap_or("manual")
        .to_string();
    tx.execute(
        "UPDATE spans SET text = ?2, origin = ?3, confidence = NULL, revision = ?4 WHERE id = ?1",
        params![span.to_string(), text, origin, next as i64],
    )
    .map_err(db)?;
    tx.execute(
        "UPDATE pages SET text_revision = text_revision + 1 WHERE page_index = (SELECT page_index FROM spans WHERE id = ?1)",
        params![span.to_string()],
    )
    .map_err(db)?;
    Ok(next)
}

fn span_page(tx: &Connection, span: SpanId) -> Result<PageIndex> {
    tx.query_row(
        "SELECT page_index FROM spans WHERE id = ?1",
        params![span.to_string()],
        |r| r.get::<_, i64>(0),
    )
    .map(|p| PageIndex(p as u32))
    .map_err(db)
}

fn filter_sql(filter: &IssueFilter) -> String {
    let status = if filter.deferred_view {
        "deferred"
    } else {
        "open"
    };
    let mut sql = format!("status = '{status}' AND (score IS NULL OR score < {}) AND page_index IN (SELECT page_index FROM pages WHERE status NOT IN ('unprocessed', 'excluded'))", filter.threshold);
    if !filter.exclude_kinds.is_empty() {
        let kinds: Vec<String> = filter
            .exclude_kinds
            .iter()
            .map(|k| format!("'{}'", k.as_str()))
            .collect();
        sql.push_str(&format!(" AND kind NOT IN ({})", kinds.join(",")));
    }
    sql
}

impl Project {
    // ----- issue index -----

    /// Rebuild a page's issues from its current spans and proposals,
    /// carrying earlier decisions over by their key (REV-06) and
    /// re-applying accepted readings to the fresh text (A-09).
    pub fn rebuild_issues(&mut self, page: PageIndex) -> Result<()> {
        self.require_write()?;
        let spans = self.page_spans(page)?;
        let proposals: Vec<sbwb_text::Proposal> = self
            .page_proposals(page)?
            .into_iter()
            .filter(|p| p.status == "open" || p.status == "applied_auto")
            .map(|p| sbwb_text::Proposal {
                id: p.id,
                span: p.span,
                span_revision: p.span_revision,
                kind: match p.kind.as_str() {
                    "hyphen_join" => sbwb_text::ProposalKind::HyphenJoin,
                    "ocr_confusion" => sbwb_text::ProposalKind::OcrConfusion,
                    "proper_name" => sbwb_text::ProposalKind::ProperName,
                    _ => sbwb_text::ProposalKind::Spelling,
                },
                original: p.original,
                replacement: p.replacement,
                score: p.score,
                reason: p.reason,
                auto_applied: p.status == "applied_auto",
                merged_span: p.merged_span,
                cross_page: p.cross_page,
            })
            .collect();
        let previous = self.issues_for_page(page)?;
        let mut fresh = build_issues(page, &spans, &proposals);
        // Structural issues from the layout (LAY-03, M8.2).
        if let Some(layout) = self.page_layout(page)? {
            let regions: Vec<sbwb_layout::Region> =
                serde_json::from_value(layout.regions).unwrap_or_default();
            let uncovered: Vec<sbwb_core::Rect> = layout
                .report
                .get("uncovered_text")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let row = self.pages()?.into_iter().find(|r| r.index == page);
            let (pw, ph) = row
                .map(|r| (r.width_pt.unwrap_or(612.0), r.height_pt.unwrap_or(792.0)))
                .unwrap_or((612.0, 792.0));
            let base = spans.len() as u32 + 1000;
            fresh.extend(sbwb_review::build_structural_issues(
                page, pw, ph, &regions, &uncovered, &spans, base,
            ));
        }
        let tx = self.conn.transaction().map_err(db)?;
        // Decisions to carry over, keyed; each is consumed once.
        let mut carry: Vec<(String, Issue)> = previous
            .iter()
            .filter(|i| matches!(i.status, IssueStatus::Resolved | IssueStatus::Deferred))
            .map(|i| {
                (
                    decision_key(i.kind, &i.original, i.replacement.as_deref()),
                    i.clone(),
                )
            })
            .collect();
        let mut matched_prev: Vec<IssueId> = Vec::new();
        for issue in fresh.iter_mut() {
            let key = decision_key(issue.kind, &issue.original, issue.replacement.as_deref());
            let Some(pos) = carry.iter().position(|(k, _)| *k == key) else {
                continue;
            };
            let (_, prev) = carry.remove(pos);
            matched_prev.push(prev.id);
            issue.status = prev.status;
            issue.decision = prev.decision.clone();
            issue.note = prev.note.clone();
            if let Some(d) = &prev.decision {
                let applied = d
                    .strip_prefix("accept:")
                    .map(|t| (t, SpanOrigin::Accepted))
                    .or_else(|| d.strip_prefix("edit:").map(|t| (t, SpanOrigin::Manual)));
                if let Some((text, origin)) = applied {
                    if text != issue.original {
                        let rev =
                            update_span_text(&tx, issue.span, text, origin, issue.span_revision)?;
                        issue.span_revision = rev;
                        if let Some(p) = issue.proposal {
                            tx.execute("UPDATE proposals SET status = 'accepted', decided_at = ?2 WHERE id = ?1", params![p.to_string(), now()]).map_err(db)?;
                        }
                    }
                } else if d == "skip" {
                    if let Some(p) = issue.proposal {
                        tx.execute("UPDATE proposals SET status = 'rejected', decided_at = ?2 WHERE id = ?1", params![p.to_string(), now()]).map_err(db)?;
                    }
                } else if d == "later" {
                    if let Some(p) = issue.proposal {
                        tx.execute(
                            "UPDATE proposals SET status = 'deferred' WHERE id = ?1",
                            params![p.to_string()],
                        )
                        .map_err(db)?;
                    }
                }
            }
        }
        // User flags survive a rerun when their span text still exists.
        for prev in previous
            .iter()
            .filter(|i| i.kind == IssueKind::UserFlag && i.status == IssueStatus::Open)
        {
            if let Some(s) = spans.iter().find(|s| s.text == prev.original) {
                let mut f = prev.clone();
                f.span = s.id;
                f.span_revision = s.revision;
                f.seq = s.seq;
                fresh.push(f);
                matched_prev.push(prev.id);
            }
        }
        // Unmatched earlier decisions become stale (kept as history).
        for prev in &previous {
            if matched_prev.contains(&prev.id) {
                tx.execute(
                    "DELETE FROM issues WHERE id = ?1",
                    params![prev.id.to_string()],
                )
                .map_err(db)?;
            } else if matches!(
                prev.status,
                IssueStatus::Resolved | IssueStatus::Deferred | IssueStatus::Stale
            ) {
                tx.execute(
                    "UPDATE issues SET status = 'stale' WHERE id = ?1",
                    params![prev.id.to_string()],
                )
                .map_err(db)?;
            } else {
                tx.execute(
                    "DELETE FROM issues WHERE id = ?1",
                    params![prev.id.to_string()],
                )
                .map_err(db)?;
            }
        }
        for issue in &fresh {
            insert_issue(&tx, issue)?;
        }
        tx.commit().map_err(db)
    }

    pub fn issues_for_page(&self, page: PageIndex) -> Result<Vec<Issue>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {ISSUE_COLS} FROM issues WHERE page_index = ?1 ORDER BY seq, created_at"
            ))
            .map_err(db)?;
        let rows = stmt
            .query_map(params![page.0 as i64], read_issue)
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    pub fn issue(&self, id: IssueId) -> Result<Option<Issue>> {
        self.conn
            .query_row(
                &format!("SELECT {ISSUE_COLS} FROM issues WHERE id = ?1"),
                params![id.to_string()],
                read_issue,
            )
            .optional()
            .map_err(db)
    }

    pub fn issue_counts(&self, filter: &IssueFilter) -> Result<IssueCounts> {
        let mut c = IssueCounts::default();
        let count = |sql: &str| -> Result<u32> {
            self.conn
                .query_row(sql, [], |r| r.get::<_, i64>(0))
                .map(|n| n as u32)
                .map_err(db)
        };
        let in_scope = "page_index IN (SELECT page_index FROM pages WHERE status NOT IN ('unprocessed', 'excluded'))";
        c.unresolved = count(&format!(
            "SELECT count(*) FROM issues WHERE status = 'open' AND {in_scope}"
        ))?;
        let open_filter = IssueFilter {
            deferred_view: false,
            ..filter.clone()
        };
        c.matching = count(&format!(
            "SELECT count(*) FROM issues WHERE {}",
            filter_sql(&open_filter)
        ))?;
        c.above_threshold = count(&format!("SELECT count(*) FROM issues WHERE status = 'open' AND score IS NOT NULL AND score >= {} AND {in_scope}", filter.threshold))?;
        if !filter.exclude_kinds.is_empty() {
            let kinds: Vec<String> = filter
                .exclude_kinds
                .iter()
                .map(|k| format!("'{}'", k.as_str()))
                .collect();
            c.filtered_kind = count(&format!(
                "SELECT count(*) FROM issues WHERE status = 'open' AND kind IN ({}) AND {in_scope}",
                kinds.join(",")
            ))?;
        }
        c.deferred = count("SELECT count(*) FROM issues WHERE status = 'deferred'")?;
        c.resolved = count("SELECT count(*) FROM issues WHERE status = 'resolved'")?;
        c.stale = count("SELECT count(*) FROM issues WHERE status = 'stale'")?;
        c.flagged =
            count("SELECT count(*) FROM issues WHERE status = 'open' AND kind = 'user_flag'")?;
        let meta = self.meta()?;
        c.pages_in_scope = meta.scope.len();
        c.unprocessed_pages = self
            .pages()?
            .iter()
            .filter(|p| meta.scope.contains(p.index) && !p.text_done)
            .count() as u32;
        Ok(c)
    }

    /// Per-page open/matching/deferred counts for the filmstrip and rail.
    pub fn page_issue_counts(&self, filter: &IssueFilter) -> Result<Vec<PageIssueCounts>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT page_index, sum(status = 'open'), sum(status = 'open' AND (score IS NULL OR score < {})), sum(status = 'deferred') FROM issues GROUP BY page_index ORDER BY page_index",
                filter.threshold
            ))
            .map_err(db)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PageIssueCounts {
                    page: r.get::<_, i64>(0)? as u32,
                    open: r.get::<_, i64>(1)? as u32,
                    matching: r.get::<_, i64>(2)? as u32,
                    deferred: r.get::<_, i64>(3)? as u32,
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    /// The next (or previous) matching issue after `from`, in reading or
    /// priority order, wrapping around the book at most once (REV-04).
    /// With `from = None` the first matching issue is returned.
    pub fn next_issue(
        &self,
        from: Option<IssueId>,
        from_page: Option<PageIndex>,
        forward: bool,
        filter: &IssueFilter,
    ) -> Result<NextIssue> {
        let order = if filter.by_priority {
            if forward {
                "priority DESC, page_index, seq, created_at"
            } else {
                "priority, page_index DESC, seq DESC, created_at DESC"
            }
        } else if forward {
            "page_index, seq, created_at"
        } else {
            "page_index DESC, seq DESC, created_at DESC"
        };
        let base = filter_sql(filter);
        let anchor = match from {
            Some(id) => self.issue(id)?,
            None => None,
        };
        // With no selected issue, start from the current page in reading
        // order (an anchor just before its first word).
        let start: Option<(i64, i64, i64)> = match (&anchor, from_page) {
            (Some(a), _) => Some((a.page.0 as i64, a.seq as i64, a.priority as i64)),
            (None, Some(p)) if !filter.by_priority => {
                Some((p.0 as i64, if forward { -1 } else { i64::MAX }, 0))
            }
            _ => None,
        };
        let position = start.map(|(p, s, pr)| {
            if filter.by_priority {
                if forward {
                    format!("(priority < {pr} OR (priority = {pr} AND (page_index > {p} OR (page_index = {p} AND seq > {s}))))")
                } else {
                    format!("(priority > {pr} OR (priority = {pr} AND (page_index < {p} OR (page_index = {p} AND seq < {s}))))")
                }
            } else if forward {
                format!("(page_index > {p} OR (page_index = {p} AND seq > {s}))")
            } else {
                format!("(page_index < {p} OR (page_index = {p} AND seq < {s}))")
            }
        });
        let query = |extra: Option<&str>| -> Result<Option<Issue>> {
            let sql = match extra {
                Some(e) => format!(
                    "SELECT {ISSUE_COLS} FROM issues WHERE {base} AND {e} ORDER BY {order} LIMIT 1"
                ),
                None => {
                    format!("SELECT {ISSUE_COLS} FROM issues WHERE {base} ORDER BY {order} LIMIT 1")
                }
            };
            self.conn
                .query_row(&sql, [], read_issue)
                .optional()
                .map_err(db)
        };
        if let Some(pos) = position.as_deref() {
            if let Some(i) = query(Some(pos))? {
                return Ok(NextIssue {
                    issue: Some(i),
                    wrapped: false,
                });
            }
        }
        let first = query(None)?;
        match (first, anchor) {
            (Some(i), Some(a)) if i.id == a.id => Ok(NextIssue {
                issue: None,
                wrapped: true,
            }),
            (Some(i), Some(_)) => Ok(NextIssue {
                issue: Some(i),
                wrapped: true,
            }),
            (first, None) => Ok(NextIssue {
                issue: first,
                wrapped: false,
            }),
            (None, _) => Ok(NextIssue {
                issue: None,
                wrapped: true,
            }),
        }
    }

    // ----- decisions -----

    /// Record a decision on an issue (REV-03, REV-06). Accept and Edit
    /// change the span text; Skip keeps the original; Later defers. All of
    /// it is one transaction with a named, undoable history entry.
    pub fn decide_issue(
        &mut self,
        id: IssueId,
        decision: &Decision,
        expected_span_revision: u64,
    ) -> Result<DecisionOutcome> {
        self.require_write()?;
        let issue = self
            .issue(id)?
            .ok_or_else(|| SbwbError::NotFound("issue not found".into()))?;
        if !matches!(issue.status, IssueStatus::Open | IssueStatus::Deferred) {
            return Err(SbwbError::Conflict("this issue was already decided".into()));
        }
        if issue.bbox.is_some() && issue.candidates.is_empty() {
            return self.decide_structural(issue, decision);
        }
        let page = span_page(&self.conn, issue.span)?;
        let old: (String, String, Option<f64>, i64) = self
            .conn
            .query_row(
                "SELECT text, origin, confidence, revision FROM spans WHERE id = ?1",
                params![issue.span.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(db)?;
        if old.3 as u64 != expected_span_revision {
            return Err(SbwbError::Conflict(
                "this word changed since the issue was loaded; reload the page".into(),
            ));
        }
        let tx = self.conn.transaction().map_err(db)?;
        let (label, decision_str, new_status, new_revision, new_text) = match decision {
            Decision::Accept { text } | Decision::Edit { text } => {
                let is_edit = matches!(decision, Decision::Edit { .. });
                let text = text.trim();
                if text.is_empty() {
                    return Err(SbwbError::InvalidInput(
                        "the reading cannot be empty".into(),
                    ));
                }
                let origin = if is_edit {
                    SpanOrigin::Manual
                } else {
                    SpanOrigin::Accepted
                };
                let rev = if text != old.0 {
                    update_span_text(&tx, issue.span, text, origin, expected_span_revision)?
                } else {
                    expected_span_revision
                };
                let accepted_proposal: Option<ProposalId> = issue
                    .candidates
                    .iter()
                    .find(|c| c.text == text)
                    .and_then(|c| c.proposal);
                for c in issue.candidates.iter().filter_map(|c| c.proposal) {
                    let status = if Some(c) == accepted_proposal {
                        "accepted"
                    } else {
                        "rejected"
                    };
                    tx.execute("UPDATE proposals SET status = ?2, decided_at = ?3 WHERE id = ?1 AND status IN ('open', 'deferred')", params![c.to_string(), status, now()]).map_err(db)?;
                }
                (
                    if text == old.0 {
                        format!("Confirmed “{}” · page {}", old.0, page.number())
                    } else {
                        format!(
                            "{} “{}” → “{}” · page {}",
                            if is_edit { "Edited" } else { "Accepted" },
                            old.0,
                            text,
                            page.number()
                        )
                    },
                    format!("{}:{}", if is_edit { "edit" } else { "accept" }, text),
                    IssueStatus::Resolved,
                    rev,
                    text.to_string(),
                )
            }
            Decision::Skip => {
                if let Some(p) = issue.proposal {
                    tx.execute(
                        "UPDATE proposals SET status = 'rejected', decided_at = ?2 WHERE id = ?1",
                        params![p.to_string(), now()],
                    )
                    .map_err(db)?;
                }
                (
                    format!("Kept “{}” · page {}", old.0, page.number()),
                    "skip".to_string(),
                    IssueStatus::Resolved,
                    expected_span_revision,
                    old.0.clone(),
                )
            }
            Decision::Later => {
                if let Some(p) = issue.proposal {
                    tx.execute(
                        "UPDATE proposals SET status = 'deferred' WHERE id = ?1",
                        params![p.to_string()],
                    )
                    .map_err(db)?;
                }
                (
                    format!("Deferred “{}” · page {}", old.0, page.number()),
                    "later".to_string(),
                    IssueStatus::Deferred,
                    expected_span_revision,
                    old.0.clone(),
                )
            }
        };
        // Other open issues on the same span are superseded by a text change.
        let mut superseded: Vec<(String, String)> = Vec::new();
        if new_text != old.0 {
            let mut stmt = tx.prepare("SELECT id, status FROM issues WHERE span_id = ?1 AND id != ?2 AND status IN ('open', 'deferred')").map_err(db)?;
            let rows = stmt
                .query_map(params![issue.span.to_string(), id.to_string()], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(db)?;
            superseded = rows
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(db)?;
            drop(stmt);
            for (sid, _) in &superseded {
                tx.execute("UPDATE issues SET status = 'resolved', decision = 'superseded', decided_at = ?2, span_revision = ?3 WHERE id = ?1", params![sid, now(), new_revision as i64]).map_err(db)?;
            }
        }
        let history = HistoryId::new();
        tx.execute(
            "UPDATE issues SET status = ?2, decision = ?3, decided_at = ?4, span_revision = ?5, history_id = ?6 WHERE id = ?1",
            params![id.to_string(), new_status.as_str(), decision_str, now(), new_revision as i64, history.to_string()],
        )
        .map_err(db)?;
        let payload = serde_json::json!({
            "kind": "decision",
            "issue": id,
            "span": issue.span,
            "page": page.0,
            "old_text": old.0,
            "new_text": new_text,
            "old_origin": old.1,
            "old_confidence": old.2,
            "old_revision": expected_span_revision,
            "new_revision": new_revision,
            "old_status": issue.status.as_str(),
            "proposal": issue.proposal,
            "candidates": issue.candidates.iter().filter_map(|c| c.proposal).collect::<Vec<_>>(),
            "superseded": superseded,
        });
        tx.execute(
            "INSERT INTO history(id, ts, kind, label, payload) VALUES (?1, ?2, 'decision', ?3, ?4)",
            params![
                history.to_string(),
                now(),
                label,
                serde_json::to_string(&payload)?
            ],
        )
        .map_err(db)?;
        tx.execute(
            "DELETE FROM drafts WHERE span_id = ?1",
            params![issue.span.to_string()],
        )
        .map_err(db)?;
        tx.commit().map_err(db)?;
        let issue = self
            .issue(id)?
            .ok_or_else(|| SbwbError::NotFound("issue vanished".into()))?;
        Ok(DecisionOutcome {
            issue,
            history,
            span_text: new_text,
            span_revision: new_revision,
        })
    }

    /// Structural issues (missing text, clipping, order) have no reading to
    /// accept: Skip records "not text / acknowledged", Later defers.
    fn decide_structural(&mut self, issue: Issue, decision: &Decision) -> Result<DecisionOutcome> {
        let (status, decision_str, label) = match decision {
            Decision::Skip => (
                IssueStatus::Resolved,
                "skip".to_string(),
                format!(
                    "Acknowledged {} · page {}",
                    issue.kind.label(),
                    issue.page.number()
                ),
            ),
            Decision::Later => (
                IssueStatus::Deferred,
                "later".to_string(),
                format!(
                    "Deferred {} · page {}",
                    issue.kind.label(),
                    issue.page.number()
                ),
            ),
            _ => {
                return Err(SbwbError::InvalidInput(
                    "this issue has no reading to accept; recognise the area or skip it".into(),
                ))
            }
        };
        let history = HistoryId::new();
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "UPDATE issues SET status = ?2, decision = ?3, decided_at = ?4, history_id = ?5 WHERE id = ?1",
            params![issue.id.to_string(), status.as_str(), decision_str, now(), history.to_string()],
        )
        .map_err(db)?;
        let payload = serde_json::json!({
            "kind": "decision", "issue": issue.id, "span": issue.span, "page": issue.page.0,
            "old_text": "", "new_text": "", "old_origin": "ocr", "old_confidence": null,
            "old_revision": 0, "new_revision": 0, "old_status": issue.status.as_str(), "proposal": null, "candidates": [], "superseded": [],
        });
        tx.execute(
            "INSERT INTO history(id, ts, kind, label, payload) VALUES (?1, ?2, 'decision', ?3, ?4)",
            params![
                history.to_string(),
                now(),
                label,
                serde_json::to_string(&payload)?
            ],
        )
        .map_err(db)?;
        tx.commit().map_err(db)?;
        let issue = self
            .issue(issue.id)?
            .ok_or_else(|| SbwbError::NotFound("issue vanished".into()))?;
        Ok(DecisionOutcome {
            issue,
            history,
            span_text: String::new(),
            span_revision: 0,
        })
    }

    /// Raise a user flag on a span (REV-01).
    pub fn flag_span(&mut self, span: SpanId, note: Option<&str>) -> Result<Issue> {
        self.require_write()?;
        let (page, seq, text, rev): (i64, i64, String, i64) = self
            .conn
            .query_row(
                "SELECT page_index, seq, text, revision FROM spans WHERE id = ?1",
                params![span.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(db)?;
        let issue = Issue {
            id: IssueId::new(),
            page: PageIndex(page as u32),
            seq: seq as u32,
            span,
            span_revision: rev as u64,
            kind: IssueKind::UserFlag,
            score: None,
            score_source: "you".into(),
            priority: 95,
            proposal: None,
            original: text.clone(),
            replacement: None,
            reason: note
                .map(|n| n.to_string())
                .unwrap_or_else(|| "flagged for a closer look".into()),
            status: IssueStatus::Open,
            decision: None,
            note: note.map(|n| n.to_string()),
            candidates: vec![Candidate {
                text,
                score: None,
                source: "current".into(),
                proposal: None,
            }],
            bbox: None,
            region: None,
        };
        insert_issue(&self.conn, &issue)?;
        self.add_history(
            "flag",
            &format!("Flagged “{}” · page {}", issue.original, page + 1),
            &serde_json::json!({ "issue": issue.id, "page": page }),
        )?;
        Ok(issue)
    }

    pub fn remove_flag(&self, id: IssueId) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "DELETE FROM issues WHERE id = ?1 AND kind = 'user_flag'",
                params![id.to_string()],
            )
            .map_err(db)?;
        Ok(())
    }

    // ----- approval (REV-06) -----

    pub fn approve_page(&mut self, page: PageIndex, acknowledged_outstanding: u32) -> Result<()> {
        self.require_write()?;
        let (text_rev, layout_rev, prev_rev, prev_out): (i64, i64, Option<i64>, i64) = self
            .conn
            .query_row(
                "SELECT text_revision, layout_revision, approved_revision, approved_outstanding FROM pages WHERE page_index = ?1",
                params![page.0 as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(db)?;
        let open: i64 = self
            .conn
            .query_row("SELECT count(*) FROM issues WHERE page_index = ?1 AND status IN ('open', 'deferred')", params![page.0 as i64], |r| r.get(0))
            .map_err(db)?;
        if open as u32 > acknowledged_outstanding {
            return Err(SbwbError::Conflict(format!(
                "{open} issues on this page are unresolved; acknowledge them to approve anyway"
            )));
        }
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "UPDATE pages SET approved_revision = ?2, approved_layout_revision = ?3, approved_outstanding = ?4, approved_at = ?5 WHERE page_index = ?1",
            params![page.0 as i64, text_rev, layout_rev, open, now()],
        )
        .map_err(db)?;
        let history = HistoryId::new();
        let label = if open > 0 {
            format!("Approved page {} with {open} outstanding", page.number())
        } else {
            format!("Approved page {}", page.number())
        };
        tx.execute(
            "INSERT INTO history(id, ts, kind, label, payload) VALUES (?1, ?2, 'approve', ?3, ?4)",
            params![
                history.to_string(),
                now(),
                label,
                serde_json::to_string(&serde_json::json!({ "kind": "approve", "page": page.0, "revision": text_rev, "outstanding": open, "prev_revision": prev_rev, "prev_outstanding": prev_out }))?
            ],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }

    pub fn unapprove_page(&mut self, page: PageIndex) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute("UPDATE pages SET approved_revision = NULL, approved_layout_revision = NULL, approved_outstanding = 0, approved_at = NULL WHERE page_index = ?1", params![page.0 as i64])
            .map_err(db)?;
        self.add_history(
            "approve",
            &format!("Approval removed · page {}", page.number()),
            &serde_json::json!({ "kind": "unapprove", "page": page.0 }),
        )?;
        Ok(())
    }

    // ----- grouped corrections (REV-05) -----

    pub fn group_preview(
        &self,
        original: &str,
        replacement: &str,
        scope: &PageScope,
    ) -> Result<Vec<GroupMatch>> {
        let core = sbwb_review::core(original);
        if core.is_empty() {
            return Ok(vec![]);
        }
        let pages = self.pages()?;
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT page_index FROM spans WHERE instr(text, ?1) > 0 ORDER BY page_index")
            .map_err(db)?;
        let hits: Vec<i64> = stmt
            .query_map(params![core], |r| r.get(0))
            .map_err(db)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db)?;
        let mut out = Vec::new();
        for p in hits {
            let page = PageIndex(p as u32);
            if !scope.contains(page) {
                continue;
            }
            let approved = pages
                .iter()
                .find(|r| r.index == page)
                .map(|r| r.approved_revision.is_some())
                .unwrap_or(false);
            let spans = self.page_spans(page)?;
            out.extend(group_matches_on_page(
                &spans,
                original,
                replacement,
                approved,
            ));
        }
        Ok(out)
    }

    /// Apply selected matches atomically. If any span moved since the
    /// preview, nothing is applied and the stale spans are returned.
    pub fn group_apply(
        &mut self,
        original: &str,
        replacement: &str,
        items: &[(SpanId, u64)],
    ) -> Result<GroupOutcome> {
        self.require_write()?;
        if items.is_empty() {
            return Ok(GroupOutcome {
                applied: 0,
                history: None,
                stale: vec![],
            });
        }
        let new_core = sbwb_review::core(replacement).to_string();
        let mut stale = Vec::new();
        for (span, rev) in items {
            let current: Option<i64> = self
                .conn
                .query_row(
                    "SELECT revision FROM spans WHERE id = ?1",
                    params![span.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db)?;
            if current.map(|c| c as u64) != Some(*rev) {
                stale.push(*span);
            }
        }
        if !stale.is_empty() {
            return Ok(GroupOutcome {
                applied: 0,
                history: None,
                stale,
            });
        }
        let tx = self.conn.transaction().map_err(db)?;
        let mut undo_items = Vec::new();
        let mut pages_touched = std::collections::BTreeSet::new();
        for (span, rev) in items {
            let (page, text, origin, conf): (i64, String, String, Option<f64>) = tx
                .query_row(
                    "SELECT page_index, text, origin, confidence FROM spans WHERE id = ?1",
                    params![span.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(db)?;
            let new_text = sbwb_review::replace_core(&text, &new_core);
            let new_rev = update_span_text(&tx, *span, &new_text, SpanOrigin::Accepted, *rev)?;
            let issues: Vec<(String, String)> = {
                let mut stmt = tx.prepare("SELECT id, status FROM issues WHERE span_id = ?1 AND status IN ('open', 'deferred')").map_err(db)?;
                let rows = stmt
                    .query_map(params![span.to_string()], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(db)?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(db)?
            };
            for (iid, _) in &issues {
                tx.execute(
                    "UPDATE issues SET status = 'resolved', decision = ?2, decided_at = ?3, span_revision = ?4 WHERE id = ?1",
                    params![iid, format!("accept:{new_text}"), now(), new_rev as i64],
                )
                .map_err(db)?;
            }
            tx.execute("UPDATE proposals SET status = 'accepted', decided_at = ?2 WHERE span_id = ?1 AND status IN ('open', 'deferred') AND replacement = ?3", params![span.to_string(), now(), new_text]).map_err(db)?;
            tx.execute("UPDATE proposals SET status = 'rejected', decided_at = ?2 WHERE span_id = ?1 AND status IN ('open', 'deferred')", params![span.to_string(), now()]).map_err(db)?;
            pages_touched.insert(page);
            undo_items.push(serde_json::json!({
                "span": span, "page": page, "old_text": text, "new_text": new_text, "old_origin": origin, "old_confidence": conf,
                "old_revision": rev, "new_revision": new_rev, "issues": issues,
            }));
        }
        let history = HistoryId::new();
        let label = format!(
            "Replaced “{}” with “{}” in {} place{} on {} page{}",
            original,
            replacement,
            items.len(),
            if items.len() == 1 { "" } else { "s" },
            pages_touched.len(),
            if pages_touched.len() == 1 { "" } else { "s" }
        );
        tx.execute(
            "INSERT INTO history(id, ts, kind, label, payload) VALUES (?1, ?2, 'group', ?3, ?4)",
            params![history.to_string(), now(), label, serde_json::to_string(&serde_json::json!({ "kind": "group", "original": original, "replacement": replacement, "items": undo_items }))?],
        )
        .map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(GroupOutcome {
            applied: items.len() as u32,
            history: Some(history),
            stale: vec![],
        })
    }

    // ----- history and undo (PRJ-05) -----

    pub fn history(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, ts, kind, label, undone, payload FROM history ORDER BY ts DESC, rowid DESC LIMIT ?1")
            .map_err(db)?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                let payload: String = r.get(5)?;
                let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
                Ok(HistoryEntry {
                    id: HistoryId::parse(&r.get::<_, String>(0)?).unwrap_or_default(),
                    ts: r.get(1)?,
                    kind: r.get(2)?,
                    label: r.get(3)?,
                    undone: r.get::<_, i64>(4)? != 0,
                    undoable: matches!(
                        v.get("kind").and_then(|k| k.as_str()),
                        Some("decision") | Some("group") | Some("approve")
                    ),
                    page: v.get("page").and_then(|p| p.as_u64()).map(|p| p as u32),
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    pub fn history_for_span(&self, span: SpanId) -> Result<Vec<HistoryEntry>> {
        let all = self.history(500)?;
        let needle = span.to_string();
        let mut out = Vec::new();
        for h in all {
            let payload: String = self
                .conn
                .query_row(
                    "SELECT payload FROM history WHERE id = ?1",
                    params![h.id.to_string()],
                    |r| r.get(0),
                )
                .map_err(db)?;
            if payload.contains(&needle) {
                out.push(h);
            }
        }
        Ok(out)
    }

    /// Undo a history entry when its preconditions still hold: every span it
    /// changed must be at the revision the entry produced (PRJ-05).
    pub fn undo(&mut self, id: HistoryId) -> Result<()> {
        self.require_write()?;
        let (payload, undone, label): (String, i64, String) = self
            .conn
            .query_row(
                "SELECT payload, undone, label FROM history WHERE id = ?1",
                params![id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(db)?
            .ok_or_else(|| SbwbError::NotFound("history entry not found".into()))?;
        if undone != 0 {
            return Err(SbwbError::Conflict("already undone".into()));
        }
        let v: serde_json::Value = serde_json::from_str(&payload)?;
        let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let tx = self.conn.transaction().map_err(db)?;
        match kind {
            "decision" => {
                let item = &v;
                undo_span_item(&tx, item)?;
                let issue = item["issue"].as_str().unwrap_or("");
                let old_status = item["old_status"].as_str().unwrap_or("open");
                let new_rev = item["new_revision"].as_u64().unwrap_or(0)
                    + if item["old_text"] != item["new_text"] {
                        1
                    } else {
                        0
                    };
                tx.execute(
                    "UPDATE issues SET status = ?2, decision = NULL, decided_at = NULL, history_id = NULL, span_revision = ?3 WHERE id = ?1",
                    params![issue, old_status, new_rev as i64],
                )
                .map_err(db)?;
                if let Some(list) = item["superseded"].as_array() {
                    for s in list {
                        let (sid, st) =
                            (s[0].as_str().unwrap_or(""), s[1].as_str().unwrap_or("open"));
                        tx.execute("UPDATE issues SET status = ?2, decision = NULL, span_revision = ?3 WHERE id = ?1", params![sid, st, new_rev as i64]).map_err(db)?;
                    }
                }
                for c in item["candidates"].as_array().into_iter().flatten() {
                    if let Some(p) = c.as_str() {
                        tx.execute("UPDATE proposals SET status = 'open', decided_at = NULL WHERE id = ?1 AND status IN ('accepted', 'rejected', 'deferred')", params![p]).map_err(db)?;
                    }
                }
                if let Some(p) = item["proposal"].as_str() {
                    tx.execute("UPDATE proposals SET status = 'open', decided_at = NULL WHERE id = ?1 AND status IN ('accepted', 'rejected', 'deferred')", params![p]).map_err(db)?;
                }
            }
            "group" => {
                let items = v["items"].as_array().cloned().unwrap_or_default();
                // Guard first: all or nothing.
                for item in &items {
                    let span = item["span"].as_str().unwrap_or("");
                    let want = item["new_revision"].as_i64().unwrap_or(-1);
                    let cur: Option<i64> = tx
                        .query_row(
                            "SELECT revision FROM spans WHERE id = ?1",
                            params![span],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(db)?;
                    if cur != Some(want) {
                        return Err(SbwbError::Conflict(format!(
                            "“{}” changed again since this correction; undo blocked",
                            item["new_text"].as_str().unwrap_or("")
                        )));
                    }
                }
                for item in &items {
                    undo_span_item(&tx, item)?;
                    let new_rev = item["new_revision"].as_u64().unwrap_or(0) + 1;
                    for i in item["issues"].as_array().into_iter().flatten() {
                        let (iid, st) =
                            (i[0].as_str().unwrap_or(""), i[1].as_str().unwrap_or("open"));
                        tx.execute("UPDATE issues SET status = ?2, decision = NULL, decided_at = NULL, span_revision = ?3 WHERE id = ?1", params![iid, st, new_rev as i64]).map_err(db)?;
                    }
                    tx.execute("UPDATE proposals SET status = 'open', decided_at = NULL WHERE span_id = ?1 AND status IN ('accepted', 'rejected')", params![item["span"].as_str().unwrap_or("")]).map_err(db)?;
                }
            }
            "approve" => {
                let page = v["page"].as_i64().unwrap_or(-1);
                match v["prev_revision"].as_i64() {
                    Some(r) => tx.execute("UPDATE pages SET approved_revision = ?2, approved_outstanding = ?3 WHERE page_index = ?1", params![page, r, v["prev_outstanding"].as_i64().unwrap_or(0)]).map_err(db)?,
                    None => tx.execute("UPDATE pages SET approved_revision = NULL, approved_layout_revision = NULL, approved_outstanding = 0, approved_at = NULL WHERE page_index = ?1", params![page]).map_err(db)?,
                };
            }
            _ => {
                return Err(SbwbError::InvalidInput(
                    "this entry cannot be undone".into(),
                ))
            }
        }
        tx.execute(
            "UPDATE history SET undone = 1 WHERE id = ?1",
            params![id.to_string()],
        )
        .map_err(db)?;
        tx.execute(
            "INSERT INTO history(id, ts, kind, label, payload) VALUES (?1, ?2, 'undo', ?3, ?4)",
            params![
                HistoryId::new().to_string(),
                now(),
                format!("Undo: {label}"),
                serde_json::to_string(&serde_json::json!({ "kind": "undo", "of": id }))?
            ],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }

    // ----- drafts (PRJ-04) -----

    pub fn put_draft(&self, span: SpanId, text: &str) -> Result<()> {
        self.require_write()?;
        let page = span_page(&self.conn, span)?;
        self.conn
            .execute(
                "INSERT INTO drafts(span_id, page_index, text, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(span_id) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
                params![span.to_string(), page.0 as i64, text, now()],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn delete_draft(&self, span: SpanId) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "DELETE FROM drafts WHERE span_id = ?1",
                params![span.to_string()],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn drafts(&self, page: Option<PageIndex>) -> Result<Vec<Draft>> {
        let mut stmt = self
            .conn
            .prepare("SELECT span_id, page_index, text, updated_at FROM drafts WHERE (?1 IS NULL OR page_index = ?1) ORDER BY updated_at")
            .map_err(db)?;
        let rows = stmt
            .query_map(params![page.map(|p| p.0 as i64)], |r| {
                Ok(Draft {
                    span: SpanId::parse(&r.get::<_, String>(0)?).unwrap_or_default(),
                    page: r.get::<_, i64>(1)? as u32,
                    text: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }
}

/// Restore one span from an undo payload item (text, origin, confidence);
/// the revision moves forward so later unrelated edits are never masked.
fn undo_span_item(tx: &Transaction, item: &serde_json::Value) -> Result<()> {
    if item["old_text"] == item["new_text"] {
        return Ok(());
    }
    let span = item["span"].as_str().unwrap_or("");
    let want = item["new_revision"].as_i64().unwrap_or(-1);
    let cur: Option<i64> = tx
        .query_row(
            "SELECT revision FROM spans WHERE id = ?1",
            params![span],
            |r| r.get(0),
        )
        .optional()
        .map_err(db)?;
    if cur != Some(want) {
        return Err(SbwbError::Conflict(
            "this word changed again since then; undo blocked".into(),
        ));
    }
    tx.execute(
        "UPDATE spans SET text = ?2, origin = ?3, confidence = ?4, revision = ?5 WHERE id = ?1",
        params![
            span,
            item["old_text"].as_str().unwrap_or(""),
            item["old_origin"].as_str().unwrap_or("ocr"),
            item["old_confidence"].as_f64(),
            want + 1
        ],
    )
    .map_err(db)?;
    tx.execute(
        "UPDATE pages SET text_revision = text_revision + 1 WHERE page_index = ?1",
        params![item["page"].as_i64().unwrap_or(-1)],
    )
    .map_err(db)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::{PageIndex, ProposalId};
    use sbwb_text::{Proposal, ProposalKind, Span};
    use std::path::Path;
    use std::time::Instant;

    fn create(dir: &Path, pages: u32) -> Project {
        let src = dir.join("book.pdf");
        let bytes = b"%PDF-1.4 fake".to_vec();
        std::fs::write(&src, &bytes).unwrap();
        let info = crate::project::SourceInfo {
            name: "book.pdf".into(),
            size: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            page_count: pages,
            title: Some("T".into()),
            author: None,
        };
        let p = Project::create(
            &dir.join("book.sbwb"),
            &src,
            info,
            &[],
            PageScope::all(pages),
            "test",
        )
        .unwrap();
        for i in 0..pages {
            p.set_page_status(PageIndex(i), sbwb_core::PageStatus::Done, None)
                .unwrap();
            p.set_stage_done(PageIndex(i), sbwb_core::Stage::TextPass, true)
                .unwrap();
        }
        p
    }

    fn span(page: u32, seq: u32, text: &str, conf: f32) -> Span {
        Span {
            id: SpanId::new(),
            page: PageIndex(page),
            seq,
            region: None,
            text: text.into(),
            anchors: vec![],
            origin: SpanOrigin::Ocr,
            confidence: Some(conf),
            trailing: " ".into(),
            revision: 1,
            protected: false,
            structure: Default::default(),
            paragraph_start: seq == 0,
        }
    }

    fn proposal(s: &Span, to: &str, score: u8) -> Proposal {
        Proposal {
            id: ProposalId::new(),
            span: s.id,
            span_revision: 1,
            kind: ProposalKind::Spelling,
            original: s.text.clone(),
            replacement: to.into(),
            score,
            reason: "dictionary".into(),
            auto_applied: false,
            merged_span: None,
            cross_page: false,
        }
    }

    #[test]
    fn decide_undo_and_carry_over() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = create(dir.path(), 2);
        let page = PageIndex(0);
        let spans = vec![
            span(0, 0, "The", 95.0),
            span(0, 1, "carricd", 60.0),
            span(0, 2, "hom", 40.0),
        ];
        let props = vec![proposal(&spans[1], "carried", 66)];
        p.put_page_text(page, None, &spans, &props).unwrap();
        let issues = p.issues_for_page(page).unwrap();
        assert_eq!(issues.len(), 2);
        let f = IssueFilter::default();
        let c = p.issue_counts(&f).unwrap();
        assert_eq!((c.unresolved, c.matching, c.unprocessed_pages), (2, 2, 0));
        // navigation: first, next, wrap
        let n1 = p.next_issue(None, None, true, &f).unwrap().issue.unwrap();
        let n2 = p.next_issue(Some(n1.id), None, true, &f).unwrap();
        assert!(!n2.wrapped);
        let n3 = p
            .next_issue(Some(n2.issue.unwrap().id), None, true, &f)
            .unwrap();
        assert!(n3.wrapped && n3.issue.map(|i| i.id) == Some(n1.id));
        // accept
        let out = p
            .decide_issue(
                n1.id,
                &Decision::Accept {
                    text: "carried".into(),
                },
                1,
            )
            .unwrap();
        assert_eq!(out.span_text, "carried");
        assert_eq!(out.span_revision, 2);
        let s = p.page_spans(page).unwrap();
        assert_eq!(s[1].text, "carried");
        assert_eq!(s[1].origin, SpanOrigin::Accepted);
        assert_eq!(s[1].confidence, None);
        // stale revision is refused
        let i2 = p
            .issues_for_page(page)
            .unwrap()
            .into_iter()
            .find(|i| i.status == IssueStatus::Open)
            .unwrap();
        assert!(p.decide_issue(i2.id, &Decision::Skip, 99).is_err());
        p.decide_issue(i2.id, &Decision::Later, 1).unwrap();
        assert_eq!(p.issue_counts(&f).unwrap().deferred, 1);
        // undo the accept
        p.undo(out.history).unwrap();
        let s = p.page_spans(page).unwrap();
        assert_eq!(s[1].text, "carricd");
        assert_eq!(s[1].revision, 3);
        assert_eq!(p.issue(n1.id).unwrap().unwrap().status, IssueStatus::Open);
        assert!(p.undo(out.history).is_err(), "double undo refused");
        // decide again, then rerun the page: the decision carries over
        let out = p
            .decide_issue(
                n1.id,
                &Decision::Accept {
                    text: "carried".into(),
                },
                3,
            )
            .unwrap();
        let spans2 = vec![
            span(0, 0, "The", 95.0),
            span(0, 1, "carricd", 61.0),
            span(0, 2, "hom", 40.0),
        ];
        let props2 = vec![proposal(&spans2[1], "carried", 66)];
        p.put_page_text(page, None, &spans2, &props2).unwrap();
        let s = p.page_spans(page).unwrap();
        assert_eq!(
            s[1].text, "carried",
            "accepted reading re-applied after rerun"
        );
        let issues = p.issues_for_page(page).unwrap();
        let carried = issues.iter().find(|i| i.original == "carricd").unwrap();
        assert_eq!(carried.status, IssueStatus::Resolved);
        let hom = issues.iter().find(|i| i.original == "hom").unwrap();
        assert_eq!(hom.status, IssueStatus::Deferred);
        let _ = out;
    }

    #[test]
    fn approval_and_group_apply_are_guarded() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = create(dir.path(), 2);
        let s0 = vec![
            span(0, 0, "Phenicians,", 90.0),
            span(0, 1, "and", 95.0),
            span(0, 2, "Phenicians", 91.0),
        ];
        let s1 = vec![span(1, 0, "Phenicians", 92.0), span(1, 1, "sailed", 95.0)];
        p.put_page_text(PageIndex(0), None, &s0, &[]).unwrap();
        p.put_page_text(PageIndex(1), None, &s1, &[]).unwrap();
        assert!(p.approve_page(PageIndex(1), 0).is_ok());
        let m = p
            .group_preview("Phenicians", "Phoenicians", &PageScope::all(2))
            .unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m[2].conflict.as_deref(), Some("on an approved page"));
        // one stale item blocks the whole apply
        let items: Vec<(SpanId, u64)> = vec![(m[0].span, m[0].revision), (m[1].span, 7)];
        let out = p.group_apply("Phenicians", "Phoenicians", &items).unwrap();
        assert_eq!((out.applied, out.stale.len()), (0, 1));
        assert_eq!(p.page_spans(PageIndex(0)).unwrap()[0].text, "Phenicians,");
        let items: Vec<(SpanId, u64)> =
            vec![(m[0].span, m[0].revision), (m[1].span, m[1].revision)];
        let out = p.group_apply("Phenicians", "Phoenicians", &items).unwrap();
        assert_eq!(out.applied, 2);
        let s = p.page_spans(PageIndex(0)).unwrap();
        assert_eq!(
            (s[0].text.as_str(), s[2].text.as_str()),
            ("Phoenicians,", "Phoenicians")
        );
        p.undo(out.history.unwrap()).unwrap();
        assert_eq!(p.page_spans(PageIndex(0)).unwrap()[0].text, "Phenicians,");
        // approval needs acknowledgement when issues are open
        let mut low = vec![span(1, 0, "Phenicians", 92.0), span(1, 1, "sai1ed", 30.0)];
        low[1].revision = 1;
        p.put_page_text(PageIndex(1), None, &low, &[]).unwrap();
        assert!(p.approve_page(PageIndex(1), 0).is_err());
        p.approve_page(PageIndex(1), 1).unwrap();
        let row = p
            .pages()
            .unwrap()
            .into_iter()
            .find(|r| r.index == PageIndex(1))
            .unwrap();
        assert!(row.approved_revision.is_some());
        assert_eq!(row.approved_outstanding, 1);
        let h = p.history(10).unwrap();
        assert!(h[0].label.contains("outstanding"));
        p.put_draft(low[1].id, "sailed").unwrap();
        assert_eq!(p.drafts(Some(PageIndex(1))).unwrap().len(), 1);
    }

    #[test]
    fn next_issue_is_fast_on_ten_thousand_issues() {
        // NFR-03 / M6.11: p95 of the indexed next-issue query under 300 ms.
        let dir = tempfile::tempdir().unwrap();
        let mut p = create(dir.path(), 500);
        for page in 0..500u32 {
            let spans: Vec<Span> = (0..20)
                .map(|i| span(page, i, &format!("w{page}x{i}"), 30.0 + (i as f32)))
                .collect();
            p.put_page_text(PageIndex(page), None, &spans, &[]).unwrap();
        }
        let f = IssueFilter::default();
        assert_eq!(p.issue_counts(&f).unwrap().unresolved, 10_000);
        let mut times = Vec::new();
        let mut cur = p.next_issue(None, None, true, &f).unwrap().issue;
        for _ in 0..200 {
            let t = Instant::now();
            cur = p
                .next_issue(cur.map(|i| i.id), None, true, &f)
                .unwrap()
                .issue;
            times.push(t.elapsed());
        }
        times.sort();
        let p95 = times[(times.len() as f64 * 0.95) as usize];
        eprintln!("next_issue p95 = {p95:?}");
        assert!(p95.as_millis() <= 300, "p95 {p95:?}");
        let fp = IssueFilter {
            by_priority: true,
            ..Default::default()
        };
        let t = Instant::now();
        let first = p.next_issue(None, None, true, &fp).unwrap().issue.unwrap();
        let _ = p.next_issue(Some(first.id), None, true, &fp).unwrap();
        assert!(t.elapsed().as_millis() <= 600);
    }
}
