//! Review model (REV-01..06): issues derived from spans and proposals,
//! priority, filters, and grouped-correction matching. Pure logic; the
//! store persists and indexes issues, the app exposes them.

use sbwb_core::{IssueId, PageIndex, ProposalId, SpanId};
use sbwb_text::{Proposal, ProposalKind, Span, SpanOrigin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    /// Coverage gap: text on the scan that no region covers (M8.2).
    MissingText,
    /// Column or reading-order ambiguity (uncertain regions).
    OrderAmbiguity,
    /// A region touches the page edge.
    Clipping,
    /// Low OCR confidence with no better reading proposed.
    ConflictingReadings,
    /// A spelling or confusion proposal that was not applied.
    RiskySubstitution,
    /// A hyphen join that was not applied.
    QuestionableJoin,
    /// A user flag.
    UserFlag,
}

impl IssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueKind::MissingText => "missing_text",
            IssueKind::OrderAmbiguity => "order_ambiguity",
            IssueKind::Clipping => "clipping",
            IssueKind::ConflictingReadings => "conflicting_readings",
            IssueKind::RiskySubstitution => "risky_substitution",
            IssueKind::QuestionableJoin => "questionable_join",
            IssueKind::UserFlag => "user_flag",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "missing_text" => IssueKind::MissingText,
            "order_ambiguity" => IssueKind::OrderAmbiguity,
            "clipping" => IssueKind::Clipping,
            "conflicting_readings" => IssueKind::ConflictingReadings,
            "risky_substitution" => IssueKind::RiskySubstitution,
            "questionable_join" => IssueKind::QuestionableJoin,
            "user_flag" => IssueKind::UserFlag,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::MissingText => "missing text",
            IssueKind::OrderAmbiguity => "reading order",
            IssueKind::Clipping => "clipping",
            IssueKind::ConflictingReadings => "uncertain reading",
            IssueKind::RiskySubstitution => "risky substitution",
            IssueKind::QuestionableJoin => "questionable join",
            IssueKind::UserFlag => "flagged",
        }
    }
    /// Structural issues carry no score and stay in the inbox at every
    /// threshold (REV-02).
    pub fn is_structural(self) -> bool {
        matches!(
            self,
            IssueKind::MissingText
                | IssueKind::OrderAmbiguity
                | IssueKind::Clipping
                | IssueKind::UserFlag
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Open,
    Resolved,
    Deferred,
    /// The decision no longer applies to the current text (rerun, edit).
    Stale,
}

impl IssueStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueStatus::Open => "open",
            IssueStatus::Resolved => "resolved",
            IssueStatus::Deferred => "deferred",
            IssueStatus::Stale => "stale",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "resolved" => IssueStatus::Resolved,
            "deferred" => IssueStatus::Deferred,
            "stale" => IssueStatus::Stale,
            _ => IssueStatus::Open,
        }
    }
}

/// A candidate reading shown in the Issue tab (REV-03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub text: String,
    /// Score of the proposal that produced it; `None` for the raw reading.
    pub score: Option<u8>,
    pub source: String,
    pub proposal: Option<ProposalId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub id: IssueId,
    pub page: PageIndex,
    /// Reading-order position of the span (navigation order).
    pub seq: u32,
    pub span: SpanId,
    pub span_revision: u64,
    pub kind: IssueKind,
    /// Heuristic score 0-100 for scored kinds; `None` for structural ones.
    pub score: Option<u8>,
    /// Source of the score, shown on the chip ("OCR", "text pass").
    pub score_source: String,
    /// Higher first (REV-01: impact and evidence, not confidence alone).
    pub priority: u8,
    pub proposal: Option<ProposalId>,
    /// The span text when the issue was raised.
    pub original: String,
    pub replacement: Option<String>,
    pub reason: String,
    pub status: IssueStatus,
    /// `accept:<text>`, `edit:<text>`, `skip`, `later`, `approved`, `group`.
    pub decision: Option<String>,
    pub note: Option<String>,
    pub candidates: Vec<Candidate>,
    /// Page box for structural issues that have no span (LAY-03).
    #[serde(default)]
    pub bbox: Option<sbwb_core::Rect>,
    #[serde(default)]
    pub region: Option<sbwb_core::RegionId>,
}

/// Decision key: the same original/replacement pair on the same page is
/// the same issue after a rerun (REV-06).
pub fn decision_key(kind: IssueKind, original: &str, replacement: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        kind.as_str(),
        original,
        replacement.unwrap_or("")
    )
}

/// Spans below this native confidence raise an "uncertain reading" issue
/// when nothing better was proposed. Above the review slider's range top
/// end, so the slider decides what is shown.
pub const LOW_CONFIDENCE: f32 = 90.0;

fn priority_for(kind: IssueKind, score: Option<u8>, original: &str) -> u8 {
    let len = original
        .chars()
        .filter(|c| c.is_alphanumeric())
        .count()
        .min(12) as u32;
    let uncertainty = 100u32.saturating_sub(score.unwrap_or(50) as u32);
    let p: u32 = match kind {
        IssueKind::UserFlag => 95,
        IssueKind::MissingText => 90,
        IssueKind::OrderAmbiguity => 85,
        IssueKind::Clipping => 80,
        IssueKind::QuestionableJoin => 55 + uncertainty / 5 + len,
        IssueKind::RiskySubstitution => 45 + uncertainty / 5 + len,
        IssueKind::ConflictingReadings => 20 + uncertainty / 3 + len,
    };
    p.min(100) as u8
}

/// Build the open issues for a page from its effective text and the
/// proposals that were not applied.
pub fn build_issues(page: PageIndex, spans: &[Span], proposals: &[Proposal]) -> Vec<Issue> {
    let mut out = Vec::new();
    for s in spans {
        let open: Vec<&Proposal> = proposals
            .iter()
            .filter(|p| p.span == s.id && !p.auto_applied && !p.cross_page)
            .collect();
        let raw = Candidate {
            text: s.text.clone(),
            score: s.confidence.map(|c| c.round() as u8),
            source: "raw OCR".into(),
            proposal: None,
        };
        if !open.is_empty() {
            // One issue per span: the best proposal leads, others are candidates.
            let mut sorted = open.clone();
            sorted.sort_by_key(|p| std::cmp::Reverse(p.score));
            let lead = sorted[0];
            let kind = match lead.kind {
                ProposalKind::HyphenJoin => IssueKind::QuestionableJoin,
                _ if lead.reason.contains("no word score") => IssueKind::ConflictingReadings,
                _ => IssueKind::RiskySubstitution,
            };
            let unscored = lead.reason.contains("no word score");
            let mut candidates: Vec<Candidate> = sorted
                .iter()
                .map(|p| Candidate {
                    text: p.replacement.clone(),
                    // line-level engines have no word score (OCR-02)
                    score: if p.reason.contains("no word score") {
                        None
                    } else {
                        Some(p.score)
                    },
                    source: if p.reason.contains("no word score") {
                        "second engine".into()
                    } else if p.reason.contains("region OCR") {
                        "region OCR".into()
                    } else {
                        "text pass".into()
                    },
                    proposal: Some(p.id),
                })
                .collect();
            candidates.push(raw);
            out.push(Issue {
                id: IssueId::new(),
                page,
                seq: s.seq,
                span: s.id,
                span_revision: s.revision,
                kind,
                score: if unscored { None } else { Some(lead.score) },
                score_source: if unscored {
                    "second engine".into()
                } else if lead.reason.contains("region OCR") {
                    "region OCR".into()
                } else {
                    "text pass".into()
                },
                priority: priority_for(
                    kind,
                    if unscored { None } else { Some(lead.score) },
                    &s.text,
                ),
                proposal: Some(lead.id),
                original: s.text.clone(),
                replacement: Some(lead.replacement.clone()),
                reason: lead.reason.clone(),
                status: IssueStatus::Open,
                decision: None,
                note: None,
                candidates,
                bbox: None,
                region: s.region,
            });
            continue;
        }
        if s.origin == SpanOrigin::Ocr && !s.protected {
            if let Some(c) = s.confidence {
                if c < LOW_CONFIDENCE && s.text.chars().any(|ch| ch.is_alphanumeric()) {
                    let score = c.round() as u8;
                    out.push(Issue {
                        id: IssueId::new(),
                        page,
                        seq: s.seq,
                        span: s.id,
                        span_revision: s.revision,
                        kind: IssueKind::ConflictingReadings,
                        score: Some(score),
                        score_source: "OCR".into(),
                        priority: priority_for(IssueKind::ConflictingReadings, Some(score), &s.text),
                        proposal: None,
                        original: s.text.clone(),
                        replacement: None,
                        reason: format!("OCR read this word with {score}% confidence and the dictionaries offer nothing better"),
                        status: IssueStatus::Open,
                        decision: None,
                        note: None,
                        candidates: vec![raw],
                        bbox: None,
                        region: s.region,
                    });
                }
            }
        }
    }
    out
}

/// Structural issues from the layout (LAY-03, M8.2): probable missing
/// text, regions touching the page edge, uncertain reading order. They
/// carry no score and are keyed by their rounded position so a decision
/// survives a rerun.
pub fn build_structural_issues(
    page: PageIndex,
    page_w: f64,
    page_h: f64,
    regions: &[sbwb_layout::Region],
    uncovered: &[sbwb_core::Rect],
    spans: &[Span],
    seq_base: u32,
) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut seq = seq_base;
    let mut push = |kind: IssueKind,
                    bbox: sbwb_core::Rect,
                    region: Option<sbwb_core::RegionId>,
                    reason: String,
                    seq: u32| {
        out.push(Issue {
            id: IssueId::new(),
            page,
            seq,
            span: SpanId::new(),
            span_revision: 0,
            kind,
            score: None,
            score_source: "layout".into(),
            priority: priority_for(kind, None, ""),
            proposal: None,
            original: format!("{}@{:.0},{:.0}", kind.as_str(), bbox.x, bbox.y),
            replacement: None,
            reason,
            status: IssueStatus::Open,
            decision: None,
            note: None,
            candidates: vec![],
            bbox: Some(bbox),
            region,
        });
    };
    for r in uncovered {
        // line-like, inside the page (scan-border noise sits at the edges)
        let line_like = r.h >= 3.0 && r.h <= 40.0 && r.w >= 12.0;
        let inside =
            r.x >= 4.0 && r.right() <= page_w - 4.0 && r.y >= 4.0 && r.bottom() <= page_h - 4.0;
        if !line_like || !inside {
            continue;
        }
        seq += 1;
        push(IssueKind::MissingText, *r, None, "ink that looks like a text line has no recognised words; recognise this area or mark it as not text".into(), seq);
    }
    let edge = 2.0;
    for r in regions {
        if matches!(
            r.kind,
            sbwb_layout::RegionKind::Ignore | sbwb_layout::RegionKind::Illustration
        ) {
            continue;
        }
        let b = r.bbox;
        // Clipping means recognised words sit on the page edge, not that the
        // region box (which follows ink and scan borders) does.
        let words_in: Vec<&Span> = spans.iter().filter(|s| s.region == Some(r.id)).collect();
        let touches = words_in.iter().flat_map(|s| s.anchors.iter()).any(|a| {
            let w = a.bbox;
            w.x <= edge || w.y <= edge || w.right() >= page_w - edge || w.bottom() >= page_h - edge
        });
        if touches {
            seq += 1;
            push(
                IssueKind::Clipping,
                b,
                Some(r.id),
                format!(
                    "the {} region touches the page edge; text may be cut off in the scan",
                    r.kind.label()
                ),
                seq,
            );
        }
        if r.kind == sbwb_layout::RegionKind::Uncertain && !words_in.is_empty() {
            seq += 1;
            push(IssueKind::OrderAmbiguity, b, Some(r.id), "this region's class and reading-order position are uncertain; set its type in Layout mode".into(), seq);
        }
    }
    out
}

/// Filters for navigation and counts (REV-02, REV-04).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueFilter {
    /// Scored issues strictly below this value match.
    pub threshold: u8,
    /// Kinds excluded from the inbox.
    #[serde(default)]
    pub exclude_kinds: Vec<IssueKind>,
    /// Visit deferred items instead of open ones (the deferred view).
    #[serde(default)]
    pub deferred_view: bool,
    /// Priority order instead of reading order.
    #[serde(default)]
    pub by_priority: bool,
}

impl Default for IssueFilter {
    fn default() -> Self {
        Self {
            threshold: 70,
            exclude_kinds: vec![],
            deferred_view: false,
            by_priority: false,
        }
    }
}

impl IssueFilter {
    /// Whether an issue is in the current working set.
    pub fn matches(&self, issue: &Issue) -> bool {
        let status_ok = if self.deferred_view {
            issue.status == IssueStatus::Deferred
        } else {
            issue.status == IssueStatus::Open
        };
        if !status_ok || self.exclude_kinds.contains(&issue.kind) {
            return false;
        }
        match issue.score {
            Some(s) => s < self.threshold,
            None => true,
        }
    }
}

/// Book-wide counts that explain every excluded category (REV-01).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IssueCounts {
    pub unresolved: u32,
    pub matching: u32,
    pub above_threshold: u32,
    pub filtered_kind: u32,
    pub deferred: u32,
    pub resolved: u32,
    pub stale: u32,
    pub flagged: u32,
    /// Pages in scope whose text pass has not run: their issues are unknown.
    pub unprocessed_pages: u32,
    pub pages_in_scope: u32,
}

/// Strip punctuation around a token (same rule as the text pass).
pub fn core(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
}

/// Replace the core of `original` with `core_new`, keeping surrounding
/// punctuation.
pub fn replace_core(original: &str, core_new: &str) -> String {
    let c = core(original);
    match original.find(c) {
        Some(at) if !c.is_empty() => format!(
            "{}{}{}",
            &original[..at],
            core_new,
            &original[at + c.len()..]
        ),
        _ => core_new.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupMatch {
    pub span: SpanId,
    pub page: PageIndex,
    pub seq: u32,
    pub revision: u64,
    pub text: String,
    pub replacement: String,
    pub context: String,
    /// Why the match is excluded by default; `None` when eligible.
    pub conflict: Option<String>,
}

/// Whole-token matches of `original` on a page (REV-05). Case and inner
/// punctuation must be identical; surrounding punctuation is kept.
pub fn group_matches_on_page(
    spans: &[Span],
    original: &str,
    replacement: &str,
    approved: bool,
) -> Vec<GroupMatch> {
    let want = core(original);
    if want.is_empty() {
        return vec![];
    }
    let new_core = core(replacement);
    let mut out = Vec::new();
    for (i, s) in spans.iter().enumerate() {
        if core(&s.text) != want {
            continue;
        }
        let lo = i.saturating_sub(4);
        let hi = (i + 5).min(spans.len());
        let context = spans[lo..hi]
            .iter()
            .map(|x| {
                if x.id == s.id {
                    format!("[{}]", x.text)
                } else {
                    x.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let conflict = if s.protected {
            Some("protected word".to_string())
        } else if matches!(
            s.origin,
            SpanOrigin::Manual | SpanOrigin::Accepted | SpanOrigin::Inserted
        ) {
            Some("already decided by a person".to_string())
        } else if approved {
            Some("on an approved page".to_string())
        } else {
            None
        };
        out.push(GroupMatch {
            span: s.id,
            page: s.page,
            seq: s.seq,
            revision: s.revision,
            text: s.text.clone(),
            replacement: replace_core(&s.text, new_core),
            context,
            conflict,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::RunId;

    fn span(text: &str, seq: u32, conf: Option<f32>) -> Span {
        Span {
            id: SpanId::new(),
            page: PageIndex(3),
            seq,
            region: None,
            text: text.into(),
            anchors: vec![],
            origin: SpanOrigin::Ocr,
            confidence: conf,
            trailing: " ".into(),
            revision: 1,
            protected: false,
            structure: Default::default(),
            paragraph_start: seq == 0,
        }
    }

    #[test]
    fn builds_issues_from_proposals_and_low_confidence() {
        let spans = vec![
            span("The", 0, Some(96.0)),
            span("carricd", 1, Some(70.0)),
            span("hom", 2, Some(41.0)),
            span("fine", 3, Some(99.0)),
        ];
        let props = vec![Proposal {
            id: ProposalId::new(),
            span: spans[1].id,
            span_revision: 1,
            kind: ProposalKind::Spelling,
            original: "carricd".into(),
            replacement: "carried".into(),
            score: 88,
            reason: "dictionary".into(),
            auto_applied: false,
            merged_span: None,
            cross_page: false,
        }];
        let issues = build_issues(PageIndex(3), &spans, &props);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].kind, IssueKind::RiskySubstitution);
        assert_eq!(issues[0].candidates.len(), 2);
        assert_eq!(issues[0].candidates[1].source, "raw OCR");
        assert_eq!(issues[1].kind, IssueKind::ConflictingReadings);
        assert_eq!(issues[1].score, Some(41));
        assert!(issues[1].priority > issues[0].priority - 40);
        let f = IssueFilter::default();
        assert!(!f.matches(&issues[0]), "88 is not below 70");
        assert!(f.matches(&issues[1]));
    }

    #[test]
    fn group_matching_is_whole_token_and_keeps_punctuation() {
        let mut spans = vec![
            span("Phenicians,", 0, None),
            span("Phenicians", 1, None),
            span("phenicians", 2, None),
            span("Phenician", 3, None),
        ];
        spans[1].protected = true;
        let m = group_matches_on_page(&spans, "Phenicians", "Phoenicians", false);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].replacement, "Phoenicians,");
        assert_eq!(m[1].conflict.as_deref(), Some("protected word"));
        let _ = RunId::new();
    }
}
