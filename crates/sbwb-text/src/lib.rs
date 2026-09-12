//! Text reconstruction (TXT-01..03, PROV-01).
//!
//! * [`lexicon`]: Hunspell en_GB (spellbook), Webster 1913 headwords, and
//!   the project's own vocabulary and protected words.
//! * [`rules`]: conservative single-character OCR confusions (D-24).
//! * [`reconstruct`]: OCR lines in reading order → paragraphs → word spans
//!   with source anchors, plus proposals (hyphen joins, spelling) and the
//!   auto-apply policy (threshold + eligible rule + no protection).

pub mod lexicon;
pub mod reconstruct;
pub mod rules;
pub mod span;

pub use lexicon::{early_modern_variants, LanguageProfile, Lexicon, LexiconPaths};
pub use reconstruct::{
    reconstruct, AutoApplyPolicy, PageTextInput, PageTextOutput, Proposal, ProposalKind, TextStats,
};
pub use span::{Anchor, Span, SpanOrigin, WordRef};
