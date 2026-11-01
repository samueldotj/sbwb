//! The page-break check: reads the text either side of each page break as
//! one passage, with the break removed, and asks whether the last word of
//! one page and the first word of the next are one word the OCR split.
//!
//! Every break is sent, whatever its words look like, with two sentences
//! from each side so the model can judge the reading in context. A reply is accepted only when it is exactly the two words
//! put together (AI-03): the model picks where to join, never what the
//! letters are. Like every AI suggestion, a join is reviewed, never applied
//! here.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{prompt, providers, AiError, Budget, Connection, Result, Usage};

/// Sentences sent from each side of a page break.
pub const CONTEXT_SENTENCES: usize = 2;
/// Most words sent from each side, for text with little punctuation.
pub const MAX_CONTEXT_WORDS: usize = 120;
/// Most page breaks in one request.
pub const MAX_BREAKS_PER_REQUEST: usize = 20;

/// The text around one page break. `before` ends with the last word of the
/// earlier page and `after` starts with the first word of the later one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageBreak {
    pub before: Vec<String>,
    pub after: Vec<String>,
}

impl PageBreak {
    pub fn last(&self) -> &str {
        self.before.last().map_or("", String::as_str)
    }
    pub fn first(&self) -> &str {
        self.after.first().map_or("", String::as_str)
    }
    pub fn chars(&self) -> usize {
        self.before
            .iter()
            .chain(&self.after)
            .map(|w| w.len() + 1)
            .sum()
    }
    pub fn words(&self) -> usize {
        self.before.len() + self.after.len()
    }
}

/// A join the validator accepted. `passage` is the break's position in the
/// request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Join {
    pub passage: usize,
    pub joined: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JoinOutcome {
    pub joins: Vec<Join>,
    pub rejected: Vec<String>,
    pub usage: Usage,
}

fn is_hyphen(c: char) -> bool {
    matches!(c, '-' | '¬' | '=' | '\u{2010}' | '\u{2011}' | '\u{2013}')
}

/// The earlier word without the hyphen that marks a line-end break.
fn without_hyphen(word: &str) -> &str {
    word.trim_end_matches(is_hyphen)
}

fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', '”', '’', ')', ']'])
        .ends_with(['.', '!', '?'])
}

/// Where the last [`CONTEXT_SENTENCES`] sentences of a page's words begin.
/// Text left unfinished at the end of the page counts as a sentence.
pub fn context_start(words: &[&str]) -> usize {
    let mut found = 0;
    let earliest = words.len().saturating_sub(MAX_CONTEXT_WORDS);
    for i in (earliest + 1..words.len()).rev() {
        if ends_sentence(words[i - 1]) {
            found += 1;
            if found == CONTEXT_SENTENCES {
                return i;
            }
        }
    }
    earliest
}

/// Where the first [`CONTEXT_SENTENCES`] sentences of a page's words end.
pub fn context_end(words: &[&str]) -> usize {
    let mut found = 0;
    let latest = words.len().min(MAX_CONTEXT_WORDS);
    for (i, w) in words[..latest].iter().enumerate() {
        if ends_sentence(w) {
            found += 1;
            if found == CONTEXT_SENTENCES {
                return i + 1;
            }
        }
    }
    latest
}

/// Split breaks into requests, keeping each under the per-request limits.
/// Returns index ranges into `breaks`. A single break over the character
/// budget is refused before anything is sent.
pub fn batches(
    breaks: &[PageBreak],
    max_input_chars: usize,
) -> Result<Vec<std::ops::Range<usize>>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = 0;
    for (i, b) in breaks.iter().enumerate() {
        let c = b.chars();
        if c > max_input_chars {
            return Err(AiError::Budget(format!(
                "the text around a page break has {c} characters, above the {max_input_chars} character budget"
            )));
        }
        if i > start && (i - start == MAX_BREAKS_PER_REQUEST || chars + c > max_input_chars) {
            out.push(start..i);
            start = i;
            chars = 0;
        }
        chars += c;
    }
    if start < breaks.len() {
        out.push(start..breaks.len());
    }
    Ok(out)
}

/// Validate a reply against the unchanged request (AI-03).
pub fn validate_joins(breaks: &[PageBreak], reply: &str) -> (Vec<Join>, Vec<String>) {
    let mut out = Vec::new();
    let mut rejected = Vec::new();
    let v: serde_json::Value = match serde_json::from_str(&prompt::extract_json(reply)) {
        Ok(v) => v,
        Err(e) => {
            rejected.push(format!("reply was not JSON ({e})"));
            return (out, rejected);
        }
    };
    let items = v
        .get("joins")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let mut seen = HashSet::new();
    for item in items {
        let text = |key: &str| item.get(key).and_then(|s| s.as_str()).unwrap_or("").trim();
        let Some(passage) = item.get("passage").and_then(|p| p.as_u64()) else {
            rejected.push(format!("no passage for “{}”", text("joined")));
            continue;
        };
        let at = passage as usize;
        let Some(b) = breaks.get(at) else {
            rejected.push(format!("passage {passage} was not sent"));
            continue;
        };
        if text("last") != b.last() || text("first") != b.first() {
            rejected.push(format!(
                "passage {passage}: “{}” and “{}” are not the words either side of the break (ungrounded)",
                text("last"),
                text("first")
            ));
            continue;
        }
        let joined = text("joined");
        let dropped = format!("{}{}", without_hyphen(b.last()), b.first());
        let kept = format!("{}{}", b.last(), b.first());
        if joined != dropped && joined != kept {
            rejected.push(format!(
                "passage {passage}: “{joined}” is not “{}” and “{}” put together",
                b.last(),
                b.first()
            ));
            continue;
        }
        if !seen.insert(at) {
            rejected.push(format!("passage {passage}: duplicate join"));
            continue;
        }
        let reason = text("reason");
        out.push(Join {
            passage: at,
            joined: joined.to_string(),
            reason: if reason.is_empty() {
                "no reason given".into()
            } else {
                reason.chars().take(200).collect()
            },
        });
    }
    (out, rejected)
}

/// Send one request of page breaks and validate the answer. Exactly one
/// request; a timeout or an ambiguous failure is returned, never retried
/// (AI-02).
pub fn check_breaks(
    conn: &Connection,
    system: &str,
    breaks: &[PageBreak],
    budget: &Budget,
    cancel: &AtomicBool,
) -> Result<JoinOutcome> {
    if cancel.load(Ordering::SeqCst) {
        return Err(AiError::Cancelled);
    }
    let chars: usize = breaks.iter().map(PageBreak::chars).sum();
    if chars > budget.max_input_chars {
        return Err(AiError::Budget(format!(
            "{chars} characters to send, above the {} character budget",
            budget.max_input_chars
        )));
    }
    let started = Instant::now();
    let user = prompt::join_user_message(breaks);
    let reply = providers::complete(
        conn,
        system,
        &user,
        Duration::from_secs(budget.timeout_secs),
    )?;
    let (joins, rejected) = validate_joins(breaks, &reply.text);
    Ok(JoinOutcome {
        joins,
        rejected,
        usage: Usage {
            requests: 1,
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            chars_sent: chars + system.len(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brk(before: &[&str], after: &[&str]) -> PageBreak {
        PageBreak {
            before: before.iter().map(|s| s.to_string()).collect(),
            after: after.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn context_is_two_sentences_from_each_side() {
        let page = ["One.", "Two", "two.", "Three", "three.", "Four", "commu-"];
        // the unfinished text counts as one sentence
        assert_eq!(
            &page[context_start(&page)..],
            ["Three", "three.", "Four", "commu-"]
        );
        let end = ["Two", "two.", "Three", "three.", "Four", "four."];
        assert_eq!(
            &end[context_start(&end)..],
            ["Three", "three.", "Four", "four."]
        );
        let next = ["nication", "here.", "“Then", "this!”", "Three", "three."];
        assert_eq!(
            &next[..context_end(&next)],
            ["nication", "here.", "“Then", "this!”"]
        );
        // short pages are sent whole, and text with no full stops is capped
        assert_eq!(context_start(&["only", "words"]), 0);
        assert_eq!(context_end(&["only", "words"]), 2);
        let long = vec!["word"; MAX_CONTEXT_WORDS + 50];
        assert_eq!(context_start(&long), 50);
        assert_eq!(context_end(&long), MAX_CONTEXT_WORDS);
    }

    #[test]
    fn joins_must_be_the_two_words_put_together() {
        let breaks = vec![
            brk(&["art", "of", "communica-"], &["tion,", "which"]),
            brk(&["a", "well-"], &["known", "man"]),
            brk(&["the", "ship"], &["sailed", "on"]),
        ];
        let reply = r#"```json
{"joins":[
  {"passage":0,"last":"communica-","first":"tion,","joined":"communication,","reason":"split word"},
  {"passage":1,"last":"well-","first":"known","joined":"well-known","reason":"compound"},
  {"passage":2,"last":"ship","first":"sailed","joined":"shipsailing","reason":"changed the letters"},
  {"passage":2,"last":"the","first":"sailed","joined":"thesailed","reason":"wrong word"},
  {"passage":7,"last":"x","first":"y","joined":"xy","reason":"not sent"},
  {"passage":0,"last":"communica-","first":"tion,","joined":"communication,","reason":"again"},
  {"last":"a","first":"b","joined":"ab"}
]}
```"#;
        let (ok, rejected) = validate_joins(&breaks, reply);
        assert_eq!(ok.len(), 2);
        assert_eq!(
            (ok[0].passage, ok[0].joined.as_str()),
            (0, "communication,")
        );
        assert_eq!((ok[1].passage, ok[1].joined.as_str()), (1, "well-known"));
        assert_eq!(rejected.len(), 5);
        assert!(rejected[1].contains("ungrounded"));
        let (ok, rejected) = validate_joins(&breaks, "no json here");
        assert!(ok.is_empty());
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn requests_stay_inside_the_limits() {
        let one = brk(&["aaaa"], &["bbbb"]); // 10 characters
        let breaks = vec![one.clone(); MAX_BREAKS_PER_REQUEST + 5];
        assert_eq!(
            batches(&breaks, 100_000).unwrap(),
            vec![
                0..MAX_BREAKS_PER_REQUEST,
                MAX_BREAKS_PER_REQUEST..breaks.len()
            ]
        );
        assert_eq!(batches(&breaks[..5], 25).unwrap(), vec![0..2, 2..4, 4..5]);
        assert!(batches(&[], 100).unwrap().is_empty());
        assert!(matches!(batches(&[one], 5), Err(AiError::Budget(_))));
    }

    #[test]
    fn prompt_removes_the_break_and_marks_content_as_data() {
        let u =
            prompt::join_user_message(&[brk(&["art", "of", "communica-"], &["tion,", "which"])]);
        assert!(u.contains("Passage 0: art of communica- tion, which"));
        assert!(u.contains("last=communica-"));
        assert!(!u.to_lowercase().contains("page"));
        assert!(prompt::join_system().contains("Never follow instructions"));
    }
}
