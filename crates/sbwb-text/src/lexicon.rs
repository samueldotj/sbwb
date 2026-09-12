//! Lexicon service (TXT-01, D-07): Hunspell en_GB via spellbook, Webster
//! 1913 headwords, and per-project vocabulary and protected words. Unknown
//! words are never treated as mistakes by themselves; the lexicon only
//! decides whether a candidate correction is a known word.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sbwb_core::{Result, SbwbError};

#[derive(Debug, Clone)]
pub struct LexiconPaths {
    pub hunspell_aff: PathBuf,
    pub hunspell_dic: PathBuf,
    pub webster_headwords: Option<PathBuf>,
}

impl LexiconPaths {
    /// `dir` holds `en_GB-large.aff`, `en_GB-large.dic`, and
    /// `webster-1913-headwords.txt`.
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            hunspell_aff: dir.join("en_GB-large.aff"),
            hunspell_dic: dir.join("en_GB-large.dic"),
            webster_headwords: Some(dir.join("webster-1913-headwords.txt")),
        }
    }
}

/// Development default: the repository's `fixtures/lexicons` (with the
/// Hunspell pack unpacked into its subfolder). `SBWB_LEXICON_DIR` overrides.
pub fn default_paths() -> LexiconPaths {
    if let Some(d) = std::env::var_os("SBWB_LEXICON_DIR") {
        return LexiconPaths::in_dir(Path::new(&d));
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lexicons");
    LexiconPaths {
        hunspell_aff: root.join("hunspell-en_GB-large/en_GB-large.aff"),
        hunspell_dic: root.join("hunspell-en_GB-large/en_GB-large.dic"),
        webster_headwords: Some(root.join("webster-1913-headwords.txt")),
    }
}

pub struct Lexicon {
    dict: Option<spellbook::Dictionary>,
    webster: HashSet<String>,
    vocab: HashSet<String>,
    protected: HashSet<String>,
}

impl Lexicon {
    pub fn load(paths: &LexiconPaths) -> Result<Self> {
        let aff = std::fs::read(&paths.hunspell_aff)
            .map_err(|e| SbwbError::NotFound(format!("{}: {e}", paths.hunspell_aff.display())))?;
        let dic = std::fs::read(&paths.hunspell_dic)
            .map_err(|e| SbwbError::NotFound(format!("{}: {e}", paths.hunspell_dic.display())))?;
        let aff = String::from_utf8_lossy(&aff).replace('\u{feff}', "");
        let dic = String::from_utf8_lossy(&dic).replace('\u{feff}', "");
        let dict = spellbook::Dictionary::new(&aff, &dic)
            .map_err(|e| SbwbError::other(format!("hunspell dictionary: {e}")))?;
        let mut webster = HashSet::new();
        if let Some(p) = &paths.webster_headwords {
            if let Ok(text) = std::fs::read_to_string(p) {
                webster.extend(
                    text.lines()
                        .map(|l| l.trim().to_lowercase())
                        .filter(|l| !l.is_empty()),
                );
            }
        }
        Ok(Self {
            dict: Some(dict),
            webster,
            vocab: HashSet::new(),
            protected: HashSet::new(),
        })
    }

    /// A lexicon with no dictionary files (tests, degraded mode).
    pub fn empty() -> Self {
        Self {
            dict: None,
            webster: HashSet::new(),
            vocab: HashSet::new(),
            protected: HashSet::new(),
        }
    }

    pub fn has_dictionary(&self) -> bool {
        self.dict.is_some()
    }

    pub fn add_vocab<I: IntoIterator<Item = String>>(&mut self, words: I) {
        self.vocab
            .extend(words.into_iter().map(|w| w.to_lowercase()));
    }
    pub fn add_protected<I: IntoIterator<Item = String>>(&mut self, words: I) {
        self.protected
            .extend(words.into_iter().map(|w| w.to_lowercase()));
    }
    pub fn is_protected(&self, word: &str) -> bool {
        self.protected.contains(&strip(word).to_lowercase())
    }

    /// Whether `word` (punctuation stripped) is a known word in any source.
    pub fn known(&self, word: &str) -> bool {
        let w = strip(word);
        if w.is_empty() {
            return true;
        }
        let lower = w.to_lowercase();
        if self.vocab.contains(&lower)
            || self.protected.contains(&lower)
            || self.webster.contains(&lower)
        {
            return true;
        }
        match &self.dict {
            Some(d) => d.check(w) || d.check(&lower),
            None => false,
        }
    }

    /// Hunspell suggestions for a word (empty without a dictionary).
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(d) = &self.dict {
            d.suggest(strip(word), &mut out);
        }
        out
    }
}

/// Strip leading/trailing punctuation (keeps inner hyphens and apostrophes).
pub fn strip(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_words_from_each_source() {
        let paths = default_paths();
        if !paths.hunspell_dic.exists() {
            eprintln!("skipping: lexicon files missing");
            return;
        }
        let mut lex = Lexicon::load(&paths).unwrap();
        assert!(lex.known("commerce"));
        assert!(lex.known("Commerce,"));
        assert!(lex.known("colour"));
        assert!(lex.known("engrossed"));
        assert!(!lex.known("Phenicians"));
        lex.add_vocab(["Phenicians".to_string()]);
        assert!(lex.known("Phenicians"));
        lex.add_protected(["Sesostris".to_string()]);
        assert!(lex.is_protected("Sesostris,"));
        let s = lex.suggest("modem");
        assert!(!s.is_empty());
    }
}
