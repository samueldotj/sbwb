//! Per-model token prices for the consent card's cost estimate (AI-02).
//!
//! The one place prices live. Taken on 2026-10-25 from the providers'
//! official pricing pages, standard (not batch or priority) API rates in
//! USD per million tokens:
//!
//! * OpenAI: https://developers.openai.com/api/docs/pricing
//! * Anthropic: https://platform.claude.com/docs/en/about-claude/pricing
//! * Google: https://ai.google.dev/gemini-api/docs/pricing (paid tier)
//!
//! Only the short-prompt tier is listed: one request carries one page,
//! far below the long-context thresholds (Haiku 5.5 over 100k tokens,
//! Gemini Pro over 200k, OpenAI over 272k). A model that is not here, or
//! a custom id, has no price; the estimate then says so rather than guess.

use crate::Provider;

/// The date the table was checked against the pricing pages.
pub const PRICES_AS_OF: &str = "2026-10-25";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    /// USD per million input tokens.
    pub input: f64,
    /// USD per million output tokens (thinking tokens bill as output).
    pub output: f64,
}

impl Price {
    pub fn cost(self, input_tokens: usize, output_tokens: usize) -> f64 {
        (input_tokens as f64 * self.input + output_tokens as f64 * self.output) / 1_000_000.0
    }
}

struct Entry {
    provider: Provider,
    id: &'static str,
    price: Price,
    /// A price change the pricing page already announces: from this date
    /// (YYYY-MM-DD) the second price applies.
    from: Option<(&'static str, Price)>,
}

const fn p(input: f64, output: f64) -> Price {
    Price { input, output }
}

const fn e(provider: Provider, id: &'static str, input: f64, output: f64) -> Entry {
    Entry {
        provider,
        id,
        price: p(input, output),
        from: None,
    }
}

impl Entry {
    const fn changes_on(self, date: &'static str, input: f64, output: f64) -> Entry {
        Entry {
            from: Some((date, p(input, output))),
            ..self
        }
    }
}

use Provider::{Anthropic, Google, OpenAi};

const TABLE: &[Entry] = &[
    e(OpenAi, "gpt-6-astra", 10.00, 50.00),
    e(OpenAi, "gpt-6.1-sol", 2.00, 10.00),
    e(OpenAi, "gpt-6-sol", 2.00, 10.00),
    e(OpenAi, "gpt-6-luna", 0.10, 0.50),
    e(OpenAi, "gpt-5.6-sol", 4.00, 20.00),
    e(OpenAi, "gpt-5.6-terra", 2.00, 12.00),
    e(OpenAi, "gpt-5.6-luna", 0.20, 1.20),
    e(OpenAi, "gpt-5.5", 5.00, 30.00),
    e(OpenAi, "gpt-5.5-pro", 30.00, 180.00),
    e(OpenAi, "gpt-5.4", 2.50, 15.00),
    e(OpenAi, "gpt-5.4-pro", 30.00, 180.00),
    e(OpenAi, "gpt-5.4-mini", 0.75, 4.50),
    e(OpenAi, "gpt-5.4-nano", 0.20, 1.25),
    e(OpenAi, "gpt-5.2", 1.75, 14.00),
    e(OpenAi, "gpt-5.2-pro", 21.00, 168.00),
    e(OpenAi, "gpt-5.1", 1.25, 10.00),
    e(OpenAi, "gpt-5", 1.25, 10.00),
    e(OpenAi, "gpt-5-mini", 0.25, 2.00),
    e(OpenAi, "gpt-5-nano", 0.05, 0.40),
    e(OpenAi, "gpt-5-pro", 15.00, 120.00),
    e(OpenAi, "gpt-4.1", 2.00, 8.00),
    e(OpenAi, "gpt-4.1-mini", 0.40, 1.60),
    e(OpenAi, "gpt-4.1-nano", 0.10, 0.40),
    e(OpenAi, "gpt-4o", 2.50, 10.00),
    e(OpenAi, "gpt-4o-2024-05-13", 5.00, 15.00),
    e(OpenAi, "gpt-4o-mini", 0.15, 0.60),
    e(OpenAi, "o1", 15.00, 60.00),
    e(OpenAi, "o1-pro", 150.00, 600.00),
    e(OpenAi, "o3-pro", 20.00, 80.00),
    e(OpenAi, "o3", 2.00, 8.00),
    e(OpenAi, "o4-mini", 1.10, 4.40),
    e(OpenAi, "o3-mini", 1.10, 4.40),
    e(OpenAi, "gpt-4-turbo-2024-04-09", 10.00, 30.00),
    e(OpenAi, "gpt-4-0613", 30.00, 60.00),
    e(OpenAi, "gpt-3.5-turbo", 0.50, 1.50),
    e(Anthropic, "claude-fable-5-1", 10.00, 50.00),
    e(Anthropic, "claude-mythos-5-1", 10.00, 50.00),
    e(Anthropic, "claude-fable-5", 10.00, 50.00),
    e(Anthropic, "claude-mythos-5", 10.00, 50.00),
    e(Anthropic, "claude-opus-5-5", 4.00, 20.00),
    e(Anthropic, "claude-opus-5", 5.00, 25.00),
    e(Anthropic, "claude-opus-4-8", 5.00, 25.00),
    e(Anthropic, "claude-opus-4-7", 5.00, 25.00),
    e(Anthropic, "claude-opus-4-6", 5.00, 25.00),
    e(Anthropic, "claude-opus-4-5", 5.00, 25.00),
    e(Anthropic, "claude-opus-4-1", 15.00, 75.00),
    e(Anthropic, "claude-opus-4", 15.00, 75.00),
    e(Anthropic, "claude-sonnet-5-5", 2.00, 10.00),
    e(Anthropic, "claude-sonnet-5", 2.00, 10.00),
    e(Anthropic, "claude-sonnet-4-6", 3.00, 15.00),
    e(Anthropic, "claude-sonnet-4-5", 3.00, 15.00),
    e(Anthropic, "claude-sonnet-4", 3.00, 15.00),
    e(Anthropic, "claude-haiku-5-5", 0.10, 0.50),
    e(Anthropic, "claude-haiku-4-5", 1.00, 5.00),
    e(Anthropic, "claude-3-5-haiku", 0.80, 4.00),
    e(Google, "gemini-4-argon", 2.00, 10.00).changes_on("2026-12-01", 4.00, 20.00),
    e(Google, "gemini-3.8-flash", 0.75, 3.75).changes_on("2027-01-01", 1.50, 7.50),
    e(Google, "gemini-3.6-flash", 0.75, 3.75).changes_on("2027-01-01", 1.50, 7.50),
    e(Google, "gemini-3.5-flash-lite", 0.30, 2.50),
    e(Google, "gemini-3.1-flash-lite", 0.25, 1.50),
    e(Google, "gemini-3.1-pro-preview", 2.00, 12.00),
    e(Google, "gemini-3-flash-preview", 0.50, 3.00),
    e(Google, "gemini-2.5-pro", 1.25, 10.00),
    e(Google, "gemini-2.5-flash", 0.30, 2.50),
    e(Google, "gemini-2.5-flash-lite", 0.10, 0.40),
];

/// Snapshot suffixes that keep a model's base price: a date
/// (`-20251001`, `-2024-08-06`, `-0125`), a version (`-001`), or `-latest`.
fn snapshot_suffix(rest: &str) -> bool {
    let Some(s) = rest.strip_prefix('-') else {
        return rest.is_empty();
    };
    let digits = |t: &str, n: usize| t.len() == n && t.bytes().all(|b| b.is_ascii_digit());
    let dashed = s.split('-').collect::<Vec<_>>();
    s == "latest"
        || digits(s, 8)
        || digits(s, 4)
        || digits(s, 3)
        || (dashed.len() == 3
            && digits(dashed[0], 4)
            && digits(dashed[1], 2)
            && digits(dashed[2], 2))
}

/// The price for a model on `today` (YYYY-MM-DD), or `None` when the
/// table does not know it. Exact ids win; otherwise the longest listed id
/// followed only by a snapshot suffix.
pub fn price(provider: Provider, model: &str, today: &str) -> Option<Price> {
    let model = model.trim();
    let model = model.strip_prefix("models/").unwrap_or(model);
    let entry = TABLE
        .iter()
        .filter(|en| en.provider == provider)
        .filter(|en| model.strip_prefix(en.id).is_some_and(snapshot_suffix))
        .max_by_key(|en| en.id.len())?;
    Some(match entry.from {
        Some((date, later)) if today >= date => later,
        _ => entry.price,
    })
}

/// What a finished run cost at the table's prices on its date, from the
/// tokens the provider reported. A run that sent no request cost
/// nothing; `None` when the price or the token counts are unknown.
pub fn run_cost(
    provider: Provider,
    model: &str,
    date: &str,
    requests: u32,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
) -> Option<f64> {
    if requests == 0 {
        return Some(0.0);
    }
    let p = price(provider, model, date)?;
    Some(p.cost(input_tokens? as usize, output_tokens? as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-10-25";

    #[test]
    fn exact_and_snapshot_ids_resolve() {
        assert_eq!(price(OpenAi, "gpt-5.4-mini", TODAY), Some(p(0.75, 4.50)));
        assert_eq!(
            price(OpenAi, "gpt-4o-2024-08-06", TODAY),
            Some(p(2.50, 10.00))
        );
        assert_eq!(
            price(OpenAi, "gpt-4o-2024-05-13", TODAY),
            Some(p(5.00, 15.00))
        );
        assert_eq!(
            price(OpenAi, "gpt-4o-mini-2024-07-18", TODAY),
            Some(p(0.15, 0.60))
        );
        assert_eq!(
            price(Anthropic, "claude-sonnet-4-5-20250929", TODAY),
            Some(p(3.00, 15.00))
        );
        assert_eq!(
            price(Anthropic, "claude-opus-4-20250514", TODAY),
            Some(p(15.00, 75.00))
        );
        assert_eq!(
            price(Anthropic, "claude-opus-5-5", TODAY),
            Some(p(4.00, 20.00))
        );
        assert_eq!(
            price(Google, "models/gemini-2.5-flash", TODAY),
            Some(p(0.30, 2.50))
        );
        assert_eq!(
            price(OpenAi, "gpt-3.5-turbo-0125", TODAY),
            Some(p(0.50, 1.50))
        );
    }

    #[test]
    fn unknown_models_have_no_price() {
        // a newer minor version must not inherit an older one's price
        assert_eq!(price(Anthropic, "claude-opus-4-9", TODAY), None);
        assert_eq!(price(OpenAi, "gpt-5.4-mini-tts", TODAY), None);
        assert_eq!(price(OpenAi, "my-finetune", TODAY), None);
        assert_eq!(price(Google, "gemini-2.5-flash-image", TODAY), None);
        // provider matters
        assert_eq!(price(Google, "gpt-5", TODAY), None);
    }

    #[test]
    fn announced_price_changes_apply_from_their_date() {
        assert_eq!(
            price(Google, "gemini-4-argon", "2026-11-30"),
            Some(p(2.00, 10.00))
        );
        assert_eq!(
            price(Google, "gemini-4-argon", "2026-12-01"),
            Some(p(4.00, 20.00))
        );
        assert_eq!(
            price(Google, "gemini-3.8-flash", "2027-01-01"),
            Some(p(1.50, 7.50))
        );
    }

    #[test]
    fn run_cost_needs_a_price_and_reported_tokens() {
        let c = run_cost(
            OpenAi,
            "gpt-5.4-mini",
            TODAY,
            2,
            Some(1_000_000),
            Some(100_000),
        )
        .unwrap();
        assert!((c - 1.20).abs() < 1e-9);
        assert_eq!(
            run_cost(OpenAi, "my-model", TODAY, 1, Some(10), Some(10)),
            None
        );
        assert_eq!(
            run_cost(OpenAi, "gpt-5.4-mini", TODAY, 1, None, Some(10)),
            None
        );
        // failed before any request: nothing was billed
        assert_eq!(
            run_cost(OpenAi, "my-model", TODAY, 0, None, None),
            Some(0.0)
        );
        // the price on the run's date applies
        let before = run_cost(
            Google,
            "gemini-4-argon",
            "2026-11-30",
            1,
            Some(1_000_000),
            Some(0),
        )
        .unwrap();
        let after = run_cost(
            Google,
            "gemini-4-argon",
            "2026-12-01",
            1,
            Some(1_000_000),
            Some(0),
        )
        .unwrap();
        assert!((before - 2.0).abs() < 1e-9 && (after - 4.0).abs() < 1e-9);
    }

    #[test]
    fn cost_is_per_million_tokens() {
        let c = p(2.00, 10.00).cost(100_000, 20_000);
        assert!((c - 0.40).abs() < 1e-9);
    }
}
