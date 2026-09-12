//! Reconstruction of a page's text (TXT-02, TXT-03, PROV-01).

use std::collections::BTreeMap;

use sbwb_core::{PageIndex, Rect, RunId, SpanId};
use sbwb_layout::{Region, RegionKind, WordStructure};
use sbwb_ocr::OcrWord;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::lexicon::{strip, Lexicon};
use crate::rules::confusion_candidates;
use crate::span::{Anchor, Span, SpanOrigin, WordRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// Join a line- or page-broken word.
    HyphenJoin,
    /// Single-character OCR confusion whose result is a known word (D-24).
    OcrConfusion,
    /// Dictionary suggestion at small edit distance.
    Spelling,
    /// A change to a capitalised word: never automatic (TXT-03).
    ProperName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: sbwb_core::ProposalId,
    pub span: SpanId,
    pub span_revision: u64,
    pub kind: ProposalKind,
    pub original: String,
    pub replacement: String,
    /// Heuristic score 0-100 (a score, not a probability).
    pub score: u8,
    pub reason: String,
    pub auto_applied: bool,
    /// For joins: the continuation span that was (or would be) merged.
    pub merged_span: Option<SpanId>,
    /// The proposal targets a span on the previous page.
    pub cross_page: bool,
}

/// What a page leaves for the next one: its last body word when it ends
/// with a hyphen (TXT-02 cross-page joins).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingHyphen {
    pub span: SpanId,
    pub page: PageIndex,
    pub revision: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoApplyPolicy {
    pub threshold: u8,
    pub eligible: Vec<ProposalKind>,
    /// Maximum dictionary suggestion lookups per page (each is a Hunspell
    /// edit-distance search). Unknown words past the budget are still
    /// counted, just not proposed for.
    #[serde(default = "default_suggest_budget")]
    pub suggest_budget: u32,
}

fn default_suggest_budget() -> u32 {
    40
}

impl Default for AutoApplyPolicy {
    fn default() -> Self {
        Self {
            threshold: 90,
            eligible: vec![ProposalKind::HyphenJoin, ProposalKind::OcrConfusion],
            suggest_budget: default_suggest_budget(),
        }
    }
}

pub struct PageTextInput<'a> {
    pub page: PageIndex,
    pub run: RunId,
    pub words: &'a [OcrWord],
    pub regions: &'a [Region],
    pub page_w: f64,
    pub page_h: f64,
    pub previous_tail: Option<PendingHyphen>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TextStats {
    pub words: u32,
    pub paragraphs: u32,
    pub proposals: u32,
    pub auto_applied: u32,
    pub joins: u32,
    pub unknown_words: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextOutput {
    pub spans: Vec<Span>,
    pub proposals: Vec<Proposal>,
    pub stats: TextStats,
    pub tail: Option<PendingHyphen>,
}

/// Default transcription conventions (D-23): long s to s, ligatures to
/// letters (NFKC), æ and œ kept.
pub fn normalize_transcription(text: &str) -> String {
    let mut s: String = text.nfkc().collect();
    if s.contains('ſ') {
        s = s.replace('ſ', "s");
    }
    s
}

struct Line {
    words: Vec<usize>, // indices into input.words
    bbox: Rect,
}

fn inside(w: &OcrWord, r: &Region) -> bool {
    let cx = w.bbox.x + w.bbox.w / 2.0;
    let cy = w.bbox.y + w.bbox.h / 2.0;
    cx >= r.bbox.x && cx <= r.bbox.right() && cy >= r.bbox.y && cy <= r.bbox.bottom()
}

fn lines_for(indices: &[usize], words: &[OcrWord]) -> Vec<Line> {
    let mut by_line: BTreeMap<(u32, u32, u32), Vec<usize>> = BTreeMap::new();
    for &i in indices {
        let w = &words[i];
        by_line
            .entry((w.block, w.paragraph, w.line))
            .or_default()
            .push(i);
    }
    let mut lines: Vec<Line> = by_line
        .into_values()
        .map(|mut ws| {
            ws.sort_by(|&a, &b| words[a].bbox.x.partial_cmp(&words[b].bbox.x).unwrap());
            let bbox = ws
                .iter()
                .fold(words[ws[0]].bbox, |acc, &i| acc.union(&words[i].bbox));
            Line { words: ws, bbox }
        })
        .collect();
    lines.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap()
            .then(a.bbox.x.partial_cmp(&b.bbox.x).unwrap())
    });
    lines
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Paragraph starts inside a region (TXT-02): first line, indented line,
/// line after a short line, or line after a wide vertical gap.
fn paragraph_starts(lines: &[Line], words: &[OcrWord], kind: RegionKind, page_w: f64) -> Vec<bool> {
    let n = lines.len();
    let mut starts = vec![false; n];
    if n == 0 {
        return starts;
    }
    starts[0] = true;
    match kind {
        RegionKind::Header
        | RegionKind::Footer
        | RegionKind::PageNumber
        | RegionKind::Catchword
        | RegionKind::Marginalia => {
            return starts; // one paragraph per region
        }
        _ => {}
    }
    let left = median(
        lines
            .iter()
            .filter(|l| l.words.len() >= 3)
            .map(|l| l.bbox.x)
            .collect::<Vec<_>>(),
    );
    let left = if left == 0.0 {
        lines.iter().map(|l| l.bbox.x).fold(f64::MAX, f64::min)
    } else {
        left
    };
    let widest = lines.iter().map(|l| l.bbox.w).fold(0.0, f64::max);
    let char_w = median(
        lines
            .iter()
            .flat_map(|l| {
                l.words
                    .iter()
                    .map(|&i| words[i].bbox.w / words[i].text.chars().count().max(1) as f64)
            })
            .collect(),
    )
    .max(1.0);
    let indent = (1.5 * char_w).max(0.015 * page_w);
    let pitches: Vec<f64> = lines
        .windows(2)
        .map(|w| w[1].bbox.y - w[0].bbox.y)
        .filter(|p| *p > 0.0)
        .collect();
    let pitch = median(pitches);
    for i in 1..n {
        let l = &lines[i];
        let prev = &lines[i - 1];
        let prev_text = words[*prev.words.last().unwrap()].text.as_str();
        let prev_hyphen = prev_text.ends_with('-');
        let indented = l.bbox.x - left > indent;
        let short_prev = prev.bbox.w < 0.7 * widest && !prev_hyphen;
        let gap = pitch > 0.0 && (l.bbox.y - prev.bbox.y) > 1.7 * pitch;
        if indented || short_prev || gap {
            starts[i] = true;
        }
    }
    starts
}

fn is_word_like(t: &str) -> bool {
    let s = strip(t);
    s.chars().count() >= 3
        && s.chars()
            .all(|c| c.is_alphabetic() || c == '\'' || c == '’' || c == '-')
}

fn is_roman_numeral(t: &str) -> bool {
    let t = t.trim_end_matches('.');
    !t.is_empty() && t.chars().all(|c| "ivxlcdmIVXLCDM".contains(c))
}

fn capitalised(t: &str) -> bool {
    strip(t)
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

fn replace_core(original: &str, core_new: &str) -> String {
    // keep the punctuation that surrounded the stripped core
    let core = strip(original);
    match original.find(core) {
        Some(at) => format!(
            "{}{}{}",
            &original[..at],
            core_new,
            &original[at + core.len()..]
        ),
        None => core_new.to_string(),
    }
}

pub fn reconstruct(
    input: &PageTextInput,
    lexicon: &Lexicon,
    policy: &AutoApplyPolicy,
) -> PageTextOutput {
    let words = input.words;
    let mut spans: Vec<Span> = Vec::new();
    let mut proposals: Vec<Proposal> = Vec::new();
    let mut stats = TextStats::default();
    let mut assigned = vec![false; words.len()];

    let mut regions: Vec<&Region> = input.regions.iter().collect();
    regions.sort_by_key(|r| r.order);

    // Assign words to regions in reading order; leftovers form a final group.
    let mut groups: Vec<(Option<&Region>, Vec<usize>)> = Vec::new();
    for r in &regions {
        if !r.kind.is_text() {
            for (i, w) in words.iter().enumerate() {
                if !assigned[i] && inside(w, r) {
                    assigned[i] = true; // ignored/illustration words are dropped
                }
            }
            continue;
        }
        let mut idx = Vec::new();
        for (i, w) in words.iter().enumerate() {
            if !assigned[i] && !w.text.is_empty() && inside(w, r) {
                assigned[i] = true;
                idx.push(i);
            }
        }
        if !idx.is_empty() {
            groups.push((Some(r), idx));
        }
    }
    let rest: Vec<usize> = (0..words.len())
        .filter(|&i| !assigned[i] && !words[i].text.is_empty())
        .collect();
    if !rest.is_empty() {
        groups.push((None, rest));
    }

    // Emit spans line by line, paragraph by paragraph.
    let mut seq = 0u32;
    let mut pending_join: Option<(usize, bool)> = None; // (span index of hyphenated word, body?)
    let mut first_body_span: Option<usize> = None;
    let mut last_body_span_hyphen: Option<usize> = None;
    for (region, idx) in &groups {
        let kind = region.map(|r| r.kind).unwrap_or(RegionKind::Uncertain);
        let structure = region.map(|r| r.structure).unwrap_or(WordStructure::Text);
        let lines = lines_for(idx, words);
        let starts = paragraph_starts(&lines, words, kind, input.page_w);
        for (li, line) in lines.iter().enumerate() {
            let last_line = li + 1 == lines.len();
            for (wi, &i) in line.words.iter().enumerate() {
                let w = &words[i];
                let last_in_line = wi + 1 == line.words.len();
                let paragraph_end =
                    last_in_line && (last_line || starts.get(li + 1).copied().unwrap_or(true));
                let text = normalize_transcription(&w.text);
                let span = Span {
                    id: SpanId::new(),
                    page: input.page,
                    seq,
                    region: region.map(|r| r.id),
                    text: text.clone(),
                    anchors: vec![Anchor {
                        page: input.page,
                        region: region.map(|r| r.id),
                        words: vec![WordRef {
                            run: input.run,
                            index: i as u32,
                        }],
                        bbox: w.bbox,
                        approximate: false,
                    }],
                    origin: SpanOrigin::Ocr,
                    confidence: Some(w.confidence),
                    trailing: if paragraph_end {
                        "\n".into()
                    } else {
                        " ".into()
                    },
                    revision: 1,
                    protected: lexicon.is_protected(&text),
                    structure,
                    paragraph_start: wi == 0 && starts[li],
                };
                seq += 1;
                stats.words += 1;
                if starts[li] && wi == 0 {
                    stats.paragraphs += 1;
                }
                let si = spans.len();
                spans.push(span);
                if kind == RegionKind::Body && first_body_span.is_none() {
                    first_body_span = Some(si);
                }
                // hyphen join with the previous line's last word
                if wi == 0 {
                    if let Some((prev_si, _)) = pending_join.take() {
                        propose_join(
                            &mut spans,
                            &mut proposals,
                            prev_si,
                            si,
                            lexicon,
                            false,
                            &mut stats,
                        );
                    }
                }
                if last_in_line {
                    let core = strip(&text);
                    if text.ends_with('-') && core.chars().count() >= 2 && !last_line {
                        pending_join = Some((si, kind == RegionKind::Body));
                    } else if text.ends_with('-') && last_line && kind == RegionKind::Body {
                        last_body_span_hyphen = Some(si);
                    }
                }
            }
        }
        pending_join = None;
    }

    // Cross-page join from the previous page's tail (proposal only; the
    // caller applies it to the earlier page's span, TXT-02 / EXP-02).
    if let (Some(tail), Some(fi)) = (&input.previous_tail, first_body_span) {
        let b = &spans[fi];
        let first = strip(&b.text);
        if first
            .chars()
            .next()
            .map(|c| c.is_lowercase())
            .unwrap_or(false)
        {
            let core_a = tail.text.trim_end_matches('-');
            let joined = format!("{}{}", core_a, b.text);
            let joined_core = strip(&joined);
            let (score, reason) = join_score(core_a, first, joined_core, lexicon);
            proposals.push(Proposal {
                id: sbwb_core::ProposalId::new(),
                span: tail.span,
                span_revision: tail.revision,
                kind: ProposalKind::HyphenJoin,
                original: format!("{} / {}", tail.text, b.text),
                replacement: joined,
                score,
                reason: format!("word broken across pages · {reason}"),
                auto_applied: false,
                merged_span: Some(b.id),
                cross_page: true,
            });
            stats.proposals += 1;
        }
    }

    // Joins first, so spelling never looks at word fragments.
    auto_apply(&mut spans, &mut proposals, policy, &mut stats);
    let join_fragments: std::collections::HashSet<SpanId> = proposals
        .iter()
        .filter(|p| p.kind == ProposalKind::HyphenJoin && !p.auto_applied)
        .flat_map(|p| [Some(p.span), p.merged_span])
        .flatten()
        .collect();

    // Spelling proposals (D-24 rule b and dictionary suggestions).
    let mut suggest_budget: u32 = policy.suggest_budget;
    for s in spans.iter() {
        if s.protected
            || !is_word_like(&s.text)
            || s.text.ends_with('-')
            || join_fragments.contains(&s.id)
        {
            continue;
        }
        if is_roman_numeral(strip(&s.text)) {
            continue;
        }
        let core = strip(&s.text).to_string();
        if lexicon.known(&core) {
            continue;
        }
        stats.unknown_words += 1;
        let name = capitalised(&core);
        let mut best: Option<(String, u8, String, ProposalKind)> = None;
        // D-24 rule b, conservatively: exactly one single-character confusion
        // must yield a dictionary word, and the word must be long enough
        // that a three-letter neighbour is not just as likely ("tle" is
        // "the" more often than "tie").
        let known: Vec<(String, &str)> = confusion_candidates(&core)
            .into_iter()
            .filter(|(c, _)| lexicon.known(c))
            .collect();
        if let Some((cand, from)) = known.first() {
            let to = crate::rules::CONFUSIONS
                .iter()
                .find(|(f, _)| *f == *from)
                .map(|(_, t)| *t)
                .unwrap_or("");
            let (score, why) = if known.len() > 1 {
                (
                    78,
                    format!(
                        "; also possible: {}",
                        known
                            .iter()
                            .skip(1)
                            .map(|(c, _)| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            } else if core.chars().count() < 4 {
                (84, "; short word, so left as a suggestion".to_string())
            } else {
                (92, String::new())
            };
            best = Some((
                cand.clone(),
                score,
                format!("OCR confusion {from}→{to}; “{cand}” is in the dictionary{why}"),
                ProposalKind::OcrConfusion,
            ));
        }
        // Dictionary suggestion is the expensive step (Hunspell edit search),
        // so it is bounded per page and skipped for very short or all-caps
        // tokens, which rarely yield a safe suggestion anyway.
        let want_suggest = best.is_none()
            && lexicon.has_dictionary()
            && suggest_budget > 0
            && core.chars().count() >= 4
            && !core.chars().all(|c| c.is_uppercase());
        if want_suggest {
            suggest_budget -= 1;
            let sugg = lexicon.suggest(&core);
            let same_case = |c: &String| {
                c.chars().next().map(|x| x.is_uppercase())
                    == core.chars().next().map(|x| x.is_uppercase())
            };
            if let Some(cand) = sugg.iter().find(|c| same_case(c)) {
                let d = strsim::levenshtein(&core.to_lowercase(), &cand.to_lowercase());
                if d <= 2 {
                    let score = if d == 1 { 72 } else { 55 };
                    best = Some((
                        cand.clone(),
                        score,
                        format!("dictionary suggestion at edit distance {d}"),
                        ProposalKind::Spelling,
                    ));
                }
            }
        }
        if let Some((cand, mut score, reason, mut kind)) = best {
            if name {
                score = score.min(60);
                kind = ProposalKind::ProperName;
            }
            proposals.push(Proposal {
                id: sbwb_core::ProposalId::new(),
                span: s.id,
                span_revision: s.revision,
                kind,
                original: s.text.clone(),
                replacement: replace_core(&s.text, &cand),
                score,
                reason,
                auto_applied: false,
                merged_span: None,
                cross_page: false,
            });
            stats.proposals += 1;
        }
    }

    auto_apply(&mut spans, &mut proposals, policy, &mut stats);
    for (i, s) in spans.iter_mut().enumerate() {
        s.seq = i as u32;
    }

    let tail = last_body_span_hyphen.and_then(|si| {
        let s = spans
            .iter()
            .find(|s| s.seq == si as u32)
            .or_else(|| spans.last())?;
        Some(PendingHyphen {
            span: s.id,
            page: input.page,
            revision: s.revision,
            text: s.text.clone(),
        })
    });
    PageTextOutput {
        spans,
        proposals,
        stats,
        tail,
    }
}

fn auto_apply(
    spans: &mut Vec<Span>,
    proposals: &mut [Proposal],
    policy: &AutoApplyPolicy,
    stats: &mut TextStats,
) {
    for p in proposals.iter_mut() {
        if p.auto_applied
            || p.cross_page
            || p.score < policy.threshold
            || !policy.eligible.contains(&p.kind)
        {
            continue;
        }
        let Some(si) = spans.iter().position(|s| s.id == p.span) else {
            continue;
        };
        if spans[si].protected || spans[si].revision != p.span_revision {
            continue;
        }
        match p.kind {
            ProposalKind::HyphenJoin => {
                let Some(mid) = p.merged_span else { continue };
                let Some(mi) = spans.iter().position(|s| s.id == mid) else {
                    continue;
                };
                if spans[mi].protected {
                    continue;
                }
                let b = spans.remove(mi);
                let a = &mut spans[if mi < si { si - 1 } else { si }];
                a.text = p.replacement.clone();
                a.anchors.extend(b.anchors);
                a.trailing = b.trailing;
                a.origin = SpanOrigin::AutoApplied;
                a.confidence = None;
                a.revision += 1;
                stats.joins += 1;
            }
            _ => {
                let a = &mut spans[si];
                a.text = p.replacement.clone();
                a.origin = SpanOrigin::AutoApplied;
                a.confidence = None;
                a.revision += 1;
            }
        }
        p.auto_applied = true;
        stats.auto_applied += 1;
    }
}

fn join_score(a: &str, b: &str, joined: &str, lexicon: &Lexicon) -> (u8, String) {
    if lexicon.known(joined) {
        (96, format!("“{joined}” is in the dictionary"))
    } else if lexicon.known(a) && lexicon.known(b) {
        (
            88,
            format!("both “{a}” and “{b}” are words: possible compound; hyphen kept for review"),
        )
    } else {
        (70, "joined form not in the dictionary".into())
    }
}

fn propose_join(
    spans: &mut [Span],
    proposals: &mut Vec<Proposal>,
    a_i: usize,
    b_i: usize,
    lexicon: &Lexicon,
    cross_page: bool,
    stats: &mut TextStats,
) {
    let a = &spans[a_i];
    let b = &spans[b_i];
    let first = strip(&b.text);
    if first.is_empty() {
        return;
    }
    let core_a = a.text.trim_end_matches('-');
    let joined = format!("{}{}", core_a, b.text);
    let joined_core = strip(&joined);
    let (score, reason) = join_score(core_a, first, joined_core, lexicon);
    // A genuine compound keeps its hyphen and is only ever a suggestion.
    let (replacement, kind_score) = if score == 88 {
        (format!("{}-{}", core_a, b.text), 88)
    } else {
        (joined, score)
    };
    let a = &spans[a_i];
    proposals.push(Proposal {
        id: sbwb_core::ProposalId::new(),
        span: a.id,
        span_revision: a.revision,
        kind: ProposalKind::HyphenJoin,
        original: format!("{} / {}", a.text, b.text),
        replacement,
        score: if score == 88 { 88 } else { kind_score },
        reason: format!("hyphen at a line break · {reason}"),
        auto_applied: false,
        merged_span: Some(b.id),
        cross_page,
    });
    stats.proposals += 1;
}

/// Effective page text from spans (paragraphs separated by blank lines).
pub fn render_text(spans: &[Span]) -> String {
    let mut out = String::new();
    for s in spans {
        out.push_str(&s.text);
        match s.trailing.as_str() {
            "\n" => out.push_str("\n\n"),
            t => out.push_str(t),
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::Transform;

    fn word(text: &str, line: u32, x: f64, y: f64) -> OcrWord {
        OcrWord {
            block: 1,
            paragraph: 1,
            line,
            index: 0,
            bbox: Rect::new(x, y, 8.0 * text.chars().count() as f64, 10.0),
            bbox_px: Rect::default(),
            confidence: 90.0,
            text: text.into(),
        }
    }

    fn body_region() -> Region {
        Region::new(RegionKind::Body, Rect::new(0.0, 0.0, 400.0, 400.0), 0)
    }

    #[test]
    fn joins_line_broken_words_and_finds_confusions() {
        let paths = crate::lexicon::default_paths();
        if !paths.hunspell_dic.exists() {
            eprintln!("skipping: lexicon files missing");
            return;
        }
        let lex = Lexicon::load(&paths).unwrap();
        let words = vec![
            word("opened", 1, 10.0, 10.0),
            word("a", 1, 70.0, 10.0),
            word("communica-", 1, 90.0, 10.0),
            word("tion", 2, 10.0, 24.0),
            word("with", 2, 50.0, 24.0),
            word("rnodern", 2, 90.0, 24.0),
            word("Phenicians,", 2, 160.0, 24.0),
            word("well-", 2, 260.0, 24.0),
            word("known", 3, 10.0, 38.0),
            word("men.", 3, 70.0, 38.0),
        ];
        let regions = vec![body_region()];
        let input = PageTextInput {
            page: PageIndex(0),
            run: RunId::new(),
            words: &words,
            regions: &regions,
            page_w: 400.0,
            page_h: 400.0,
            previous_tail: None,
        };
        let t0 = std::time::Instant::now();
        let out = reconstruct(&input, &lex, &AutoApplyPolicy::default());
        eprintln!("reconstruct took {:?}", t0.elapsed());
        let text = render_text(&out.spans);
        eprintln!("{text}\n{:?}\n{:#?}", out.stats, out.proposals);
        assert!(
            text.starts_with("opened a communication with modern Phenicians,"),
            "{text}"
        );
        // the compound keeps its hyphen and is not auto-applied
        assert!(text.contains("well- known"), "{text}");
        let compound = out
            .proposals
            .iter()
            .find(|p| p.original.starts_with("well-"))
            .unwrap();
        assert_eq!(compound.replacement, "well-known");
        assert!(!compound.auto_applied);
        let joined = out
            .spans
            .iter()
            .find(|s| s.text == "communication")
            .unwrap();
        assert_eq!(joined.anchors.len(), 2);
        assert_eq!(joined.origin, SpanOrigin::AutoApplied);
        assert!(joined.confidence.is_none());
        let modern = out
            .proposals
            .iter()
            .find(|p| p.replacement == "modern")
            .unwrap();
        assert_eq!(modern.kind, ProposalKind::OcrConfusion);
        assert!(modern.auto_applied);
        // capitalised unknown words are never auto-corrected
        assert!(out
            .proposals
            .iter()
            .all(|p| !(p.kind == ProposalKind::ProperName && p.auto_applied)));
        assert_eq!(out.stats.auto_applied, 2);
    }

    #[test]
    fn detects_paragraphs_by_indent_and_short_line() {
        let lex = Lexicon::empty();
        let words = vec![
            word("First", 1, 10.0, 10.0),
            word("paragraph", 1, 60.0, 10.0),
            word("continues", 1, 140.0, 10.0),
            word("here", 1, 220.0, 10.0),
            word("and", 2, 10.0, 24.0),
            word("ends.", 2, 40.0, 24.0),
            word("Second", 3, 10.0, 38.0),
            word("paragraph", 3, 70.0, 38.0),
            word("continues", 3, 150.0, 38.0),
            word("on.", 3, 230.0, 38.0),
            word("Third", 4, 30.0, 52.0),
            word("indented", 4, 80.0, 52.0),
            word("paragraph", 4, 150.0, 52.0),
            word("text", 4, 230.0, 52.0),
        ];
        let regions = vec![body_region()];
        let input = PageTextInput {
            page: PageIndex(3),
            run: RunId::new(),
            words: &words,
            regions: &regions,
            page_w: 400.0,
            page_h: 400.0,
            previous_tail: None,
        };
        let out = reconstruct(&input, &lex, &AutoApplyPolicy::default());
        assert_eq!(out.stats.paragraphs, 3, "{}", render_text(&out.spans));
        assert_eq!(render_text(&out.spans).matches("\n\n").count(), 2);
    }

    #[test]
    fn real_page_reconstruction() {
        let paths = crate::lexicon::default_paths();
        let pdfium = sbwb_pdf::default_pdfium_dir();
        let tessdata = sbwb_ocr::default_tessdata_root();
        if !paths.hunspell_dic.exists()
            || !pdfium.join("pdfium.dll").exists()
            || !tessdata.join("fast/eng.traineddata").exists()
        {
            eprintln!("skipping: deps missing");
            return;
        }
        let lex = Lexicon::load(&paths).unwrap();
        let r = sbwb_pdf::Renderer::new(&pdfium).unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf");
        let hi = r
            .render(
                &fixture,
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(47), 300),
            )
            .unwrap();
        let ocr = sbwb_ocr::Engine::new(&tessdata)
            .recognize(
                &hi.image,
                &hi.transform,
                &sbwb_ocr::OcrSettings {
                    model: sbwb_ocr::ModelPack::EngFast,
                    ..Default::default()
                },
            )
            .unwrap();
        let lo = r
            .render(
                &fixture,
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(47), sbwb_layout::ANALYSIS_DPI),
            )
            .unwrap();
        let gray = image::imageops::grayscale(&lo.image);
        let t: Transform = lo.transform;
        let layout = sbwb_layout::analyze(
            &gray,
            &t,
            &sbwb_layout::LayoutInput {
                page_w: lo.page_size.0,
                page_h: lo.page_size.1,
                words: &ocr.words,
                lines: &ocr.lines,
                blocks: &ocr.blocks,
            },
            &sbwb_layout::AnalysisSettings::default(),
        );
        let input = PageTextInput {
            page: PageIndex(47),
            run: RunId::new(),
            words: &ocr.words,
            regions: &layout.regions,
            page_w: lo.page_size.0,
            page_h: lo.page_size.1,
            previous_tail: None,
        };
        let t0 = std::time::Instant::now();
        let out = reconstruct(&input, &lex, &AutoApplyPolicy::default());
        eprintln!("reconstruct took {:?}", t0.elapsed());
        let text = render_text(&out.spans);
        eprintln!("--- page 48 ---\n{text}\n{:?}", out.stats);
        for p in &out.proposals {
            eprintln!(
                "{:?} {} → {} [{}] {} {}",
                p.kind,
                p.original,
                p.replacement,
                p.score,
                if p.auto_applied { "auto" } else { "" },
                p.reason
            );
        }
        assert!(text.contains("communication with India"), "join missing");
        assert!(
            out.stats.paragraphs >= 4,
            "paragraphs {}",
            out.stats.paragraphs
        );
        assert!(out.stats.words > 250);
        // page ends with a hyphenated word ("if we" is the last line; no tail expected here)
        assert!(out
            .proposals
            .iter()
            .all(|p| !(p.kind == ProposalKind::ProperName && p.auto_applied)));
    }
}
