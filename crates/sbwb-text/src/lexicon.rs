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

/// Language profile (TXT-01, D-06): the modern British lexicon, or Early
/// Modern English where period spellings (u/v, i/j, final -e, doubled
/// consonants, -ie, -ck) count as known so they are never "corrected".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageProfile {
    #[default]
    Modern,
    EarlyModern,
}

/// Candidate modern spellings of an Early Modern form. Each rule is applied
/// alone and in the most common combinations; the caller checks whether any
/// candidate is a known word.
pub fn early_modern_variants(word: &str) -> Vec<String> {
    let w = word.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if s != w && !out.contains(&s) {
            out.push(s);
        }
    };
    // u/v by position: initial v = u, medial u = v
    let chars: Vec<char> = w.chars().collect();
    let mut uv = String::new();
    for (i, c) in chars.iter().enumerate() {
        uv.push(match (i, c) {
            (0, 'v') => 'u',
            (i, 'u') if i > 0 && i + 1 < chars.len() => 'v',
            (_, c) => *c,
        });
    }
    push(uv.clone());
    // i/j: initial i before a vowel = j; medial "ioy"/"iu"
    let ij: String = {
        let mut s = String::new();
        for (i, c) in chars.iter().enumerate() {
            let next = chars.get(i + 1).copied().unwrap_or(' ');
            s.push(
                if *c == 'i'
                    && (i == 0 || matches!(chars[i - 1], 'e' | 'a' | 'o'))
                    && "aeouy".contains(next)
                {
                    'j'
                } else {
                    *c
                },
            );
        }
        s
    };
    push(ij.clone());
    let bases = [w.clone(), uv, ij];
    for b in bases {
        // final -e
        if let Some(t) = b.strip_suffix('e') {
            if t.len() >= 2 {
                push(t.to_string());
            }
        }
        // -ie → -y, -es → -s, -ke → -c/-k, -ll → -l
        if let Some(t) = b.strip_suffix("ie") {
            push(format!("{t}y"));
        }
        if let Some(t) = b.strip_suffix("es") {
            push(format!("{t}s"));
        }
        if let Some(t) = b.strip_suffix("ke") {
            push(format!("{t}c"));
            push(format!("{t}k"));
        }
        if let Some(t) = b.strip_suffix("cke") {
            push(format!("{t}c"));
        }
        // doubled final consonant (+e): warre → war, sunne → sun
        let bc: Vec<char> = b.chars().collect();
        let n = bc.len();
        if n >= 4 && bc[n - 1] == 'e' && bc[n - 2] == bc[n - 3] && !"aeiou".contains(bc[n - 2]) {
            push(bc[..n - 2].iter().collect());
        }
        if n >= 3 && bc[n - 1] == bc[n - 2] && !"aeiou".contains(bc[n - 1]) {
            push(bc[..n - 1].iter().collect());
        }
        // ee → e (hee, bee), ay → ai (sayd), oo → o (poore), ea → ee (deere)
        if let Some(t) = b.strip_suffix("ee") {
            if t.len() <= 2 {
                push(format!("{t}e"));
            }
        }
        push(b.replace("ay", "ai"));
        push(b.replace("ayd", "aid").replace("aide", "aid"));
        push(b.replace("eere", "ear"));
        push(b.replace("oore", "oor"));
        push(b.replace("aine", "ain"));
        push(b.replace("aigne", "eign"));
        push(b.replace("eeue", "ieve").replace("eiue", "eive"));
        push(b.replace("ould", "ol"));
        push(b.replace("ersw", "ersu"));
        push(b.replace("shalbe", "shall"));
        push(b.replace("ertue", "irtue"));
        push(b.replace("oath", "oth"));
        push(b.replace("shew", "show"));
        push(b.replace("ould", "old"));
    }
    out
}

pub struct Lexicon {
    profile: LanguageProfile,
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
            profile: LanguageProfile::Modern,
            dict: Some(dict),
            webster,
            vocab: HashSet::new(),
            protected: HashSet::new(),
        })
    }

    /// A lexicon with no dictionary files (tests, degraded mode).
    pub fn empty() -> Self {
        Self {
            profile: LanguageProfile::Modern,
            dict: None,
            webster: HashSet::new(),
            vocab: HashSet::new(),
            protected: HashSet::new(),
        }
    }

    pub fn set_profile(&mut self, profile: LanguageProfile) {
        self.profile = profile;
    }
    pub fn profile(&self) -> LanguageProfile {
        self.profile
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
        let base = match &self.dict {
            Some(d) => d.check(w) || d.check(&lower),
            None => false,
        };
        if base || self.profile != LanguageProfile::EarlyModern {
            return base;
        }
        // Early Modern: a period spelling of a known word is known.
        early_modern_variants(w).iter().any(|v| {
            self.webster.contains(v) || self.dict.as_ref().map(|d| d.check(v)).unwrap_or(false)
        })
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

#[cfg(test)]
mod eme_tests {
    use super::*;

    /// P2.4 evaluation fixture: period spellings must count as known under
    /// the Early Modern profile (so the text pass never "corrects" them)
    /// and, for control, mostly unknown under the modern profile.
    #[test]
    fn early_modern_forms_are_known_under_the_profile() {
        let paths = default_paths();
        if !paths.hunspell_dic.exists() {
            eprintln!("skipping: lexicon files missing");
            return;
        }
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/lexicons/eme-sample.json");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
        let words: Vec<(String, String)> = v["words"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p[0].as_str().unwrap().to_string(),
                    p[1].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let mut lex = Lexicon::load(&paths).unwrap();
        let modern_known = words.iter().filter(|(w, _)| lex.known(w)).count();
        lex.set_profile(LanguageProfile::EarlyModern);
        let mut missed = Vec::new();
        for (w, m) in &words {
            if !lex.known(w) {
                missed.push(format!("{w} ({m})"));
            }
        }
        let known = words.len() - missed.len();
        let rate = known as f64 / words.len() as f64;
        eprintln!("EME sample: {known}/{} known under Early Modern ({:.0}%), {modern_known} under Modern; missed: {}", words.len(), rate * 100.0, missed.join(", "));
        assert!(
            rate >= 0.9,
            "Early Modern coverage {:.0}% below the 90% gate: {}",
            rate * 100.0,
            missed.join(", ")
        );
        assert!(
            modern_known < words.len() / 2,
            "the fixture should mostly be unknown to the modern lexicon"
        );
    }
}
