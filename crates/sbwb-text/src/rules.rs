//! Conservative OCR confusion rules (D-24 rule b). Each entry maps a
//! recognized fragment to what the type usually was. Only one substitution
//! is tried per candidate so the change stays explainable.

pub const CONFUSIONS: &[(&str, &str)] = &[
    ("rn", "m"),
    ("m", "rn"),
    ("l", "i"),
    ("i", "l"),
    ("1", "l"),
    ("l", "1"),
    ("I", "l"),
    ("l", "I"),
    ("0", "o"),
    ("o", "0"),
    ("c", "e"),
    ("e", "c"),
    ("ſ", "s"),
    ("f", "s"),
    ("s", "f"),
    ("u", "n"),
    ("n", "u"),
    ("cl", "d"),
    ("d", "cl"),
    ("vv", "w"),
    ("h", "b"),
    ("b", "h"),
    ("t", "l"),
    ("mn", "rnn"),
    ("nn", "m"),
    ("li", "h"),
    ("h", "li"),
];

/// Every string obtained from `word` by replacing one occurrence of one
/// confusion pattern, with the pattern that produced it.
pub fn confusion_candidates(word: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for (from, to) in CONFUSIONS {
        let mut start = 0;
        while let Some(pos) = word[start..].find(from) {
            let at = start + pos;
            let mut cand = String::with_capacity(word.len());
            cand.push_str(&word[..at]);
            cand.push_str(to);
            cand.push_str(&word[at + from.len()..]);
            if cand != word {
                out.push((cand, *from));
            }
            start = at + from.len().max(1);
            if start >= word.len() {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_single_substitutions() {
        let c = confusion_candidates("modem");
        assert!(c.iter().any(|(w, _)| w == "rnodem"));
        assert!(c.iter().any(|(w, _)| w == "modern"));
        let c = confusion_candidates("Egyptlans");
        assert!(c.iter().any(|(w, _)| w == "Egyptians"));
    }
}
