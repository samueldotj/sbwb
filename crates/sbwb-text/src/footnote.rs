//! Footnote markers (TXT-02): the marks a period book prints to call a
//! note ("*", "†", a digit), as found at the end of a word in the text and
//! at the start of a footnote, and their removal when the link is written
//! as a real footnote.

/// Markers a period book uses for notes.
const SYMBOLS: [&str; 7] = ["*", "†", "‡", "§", "‖", "¶", "||"];

/// What OCR commonly makes of a dagger or double dagger. Taken as a marker
/// only where ordinary punctuation cannot be meant: alone at the start of
/// a note, or right after other punctuation at the end of a word.
const MISREAD: [&str; 2] = ["!", "?"];

fn superscript_digit(c: char) -> Option<char> {
    match c {
        '⁰' => Some('0'),
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁴'..='⁹' => char::from_digit(c as u32 - '⁴' as u32 + 4, 10),
        _ => None,
    }
}

/// The marker a footnote paragraph starts with: "*", "(1)", "1.", "¹", or a
/// symbol glued to the first word ("*Hough").
pub fn note_marker(first: &str) -> Option<String> {
    let t = first.trim();
    let bare = t
        .trim_start_matches('(')
        .trim_end_matches(['.', ')', ','])
        .trim();
    if SYMBOLS.contains(&bare) {
        return Some(bare.to_string());
    }
    if MISREAD.contains(&t) {
        return Some(t.to_string());
    }
    if !bare.is_empty() && bare.len() <= 2 && bare.chars().all(|c| c.is_ascii_digit()) {
        return Some(bare.to_string());
    }
    if !bare.is_empty() && bare.chars().all(|c| superscript_digit(c).is_some()) {
        return Some(bare.chars().filter_map(superscript_digit).collect());
    }
    SYMBOLS
        .iter()
        .find(|m| t.starts_with(**m) && t[m.len()..].starts_with(char::is_alphabetic))
        .map(|m| m.to_string())
}

/// The marker a word in the text carries at its end ("world*", "India,†",
/// "Hough²", "Hough12"), or a word that is only a marker.
pub fn word_marker(word: &str) -> Option<String> {
    // "wisdom.!" or "Hebrews,?": a misread dagger after punctuation
    for m in MISREAD {
        if let Some(head) = word.strip_suffix(m) {
            let before = head.chars().last();
            if before
                .is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace() && c.to_string() != m)
                && head.chars().any(char::is_alphabetic)
            {
                return Some(m.to_string());
            }
        }
    }
    let t = word.trim_end_matches(['.', ',', ';', ':', ')']);
    if SYMBOLS.contains(&t) {
        return Some(t.to_string());
    }
    for m in SYMBOLS {
        if let Some(head) = t.strip_suffix(m) {
            if head.chars().any(char::is_alphabetic) {
                return Some(m.to_string());
            }
        }
    }
    let sup: String = t
        .chars()
        .rev()
        .take_while(|c| superscript_digit(*c).is_some())
        .collect();
    if !sup.is_empty() {
        return Some(sup.chars().rev().filter_map(superscript_digit).collect());
    }
    // Plain digits glued to a word: only after two or more letters, and at
    // most two digits, so years and figures are left alone.
    let digits: String = t
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let head = &t[..t.len() - digits.len()];
    let head = head.trim_end_matches([',', ';', ':', '.']);
    if !digits.is_empty()
        && digits.len() <= 2
        && head
            .chars()
            .rev()
            .take(2)
            .filter(|c| c.is_alphabetic())
            .count()
            == 2
    {
        return Some(digits);
    }
    None
}

/// Remove a printed marker from the end of a word, keeping punctuation
/// that followed it ("world*," → "world,").
pub fn strip_word_marker(word: &str, marker: &str) -> String {
    if marker.is_empty() {
        return word.to_string();
    }
    let tail_len = word.len() - word.trim_end_matches(['.', ',', ';', ':', ')']).len();
    let (core, tail) = word.split_at(word.len() - tail_len);
    const SUP: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let sup: String = marker
        .chars()
        .map(|c| c.to_digit(10).map(|d| SUP[d as usize]).unwrap_or(c))
        .collect();
    let core = core
        .strip_suffix(marker)
        .or_else(|| core.strip_suffix(sup.as_str()))
        .unwrap_or(core);
    format!("{core}{tail}")
}

/// Remove a printed marker from the start of a note ("(1)" → "", "*Hough"
/// → "Hough").
pub fn strip_note_marker(first: &str, marker: &str) -> String {
    if marker.is_empty() {
        return first.to_string();
    }
    match note_marker(first) {
        Some(m) if m == marker => {
            let t = first.trim();
            if let Some(rest) = t.strip_prefix(marker) {
                if rest.starts_with(char::is_alphabetic) {
                    return rest.to_string();
                }
            }
            String::new()
        }
        _ => first.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_in_words_and_notes() {
        assert_eq!(word_marker("world*").as_deref(), Some("*"));
        assert_eq!(word_marker("India,†").as_deref(), Some("†"));
        assert_eq!(word_marker("Hough²").as_deref(), Some("2"));
        assert_eq!(word_marker("Hough12").as_deref(), Some("12"));
        assert_eq!(word_marker("*").as_deref(), Some("*"));
        assert_eq!(word_marker("1839"), None, "years are not markers");
        assert_eq!(word_marker("A1"), None);
        assert_eq!(word_marker("plain"), None);
        assert_eq!(word_marker("wisdom.!").as_deref(), Some("!"));
        assert_eq!(word_marker("Hebrews,?").as_deref(), Some("?"));
        assert_eq!(word_marker("Lord!"), None, "an exclamation is not a marker");
        assert_eq!(word_marker("What?"), None);
        assert_eq!(note_marker("!").as_deref(), Some("!"));
        assert_eq!(note_marker("Why?"), None);
        assert_eq!(note_marker("*").as_deref(), Some("*"));
        assert_eq!(note_marker("(1)").as_deref(), Some("1"));
        assert_eq!(note_marker("2.").as_deref(), Some("2"));
        assert_eq!(note_marker("¹").as_deref(), Some("1"));
        assert_eq!(note_marker("†Asiatic").as_deref(), Some("†"));
        assert_eq!(note_marker("The"), None);
        assert_eq!(note_marker("1839"), None);
    }

    #[test]
    fn strip_markers_keeps_punctuation() {
        assert_eq!(strip_word_marker("world*,", "*"), "world,");
        assert_eq!(strip_word_marker("Hough²", "2"), "Hough");
        assert_eq!(strip_word_marker("Hough12.", "12"), "Hough.");
        assert_eq!(strip_word_marker("plain", ""), "plain");
        assert_eq!(strip_word_marker("wisdom.!", "!"), "wisdom.");
        assert_eq!(strip_note_marker("(1)", "1"), "");
        assert_eq!(strip_note_marker("†Asiatic", "†"), "Asiatic");
        assert_eq!(strip_note_marker("The", "*"), "The");
    }
}
