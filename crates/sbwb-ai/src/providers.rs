//! Provider adapters (AI-01). Each exposes model discovery and one chat
//! completion. Error classes: connection (network, timeout), auth (401/403),
//! discovery (listing failed), compatibility (the request was refused).

use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::{AiError, Connection, ModelInfo, Provider, RawReply, Result};

const OPENAI: &str = "https://api.openai.com/v1";
const ANTHROPIC: &str = "https://api.anthropic.com/v1";
const GOOGLE: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Base URL per provider; `SBWB_AI_<PROVIDER>_BASE` overrides it for
/// tests against a local stand-in server.
fn base(provider: Provider) -> String {
    let var = match provider {
        Provider::OpenAi => "SBWB_AI_OPENAI_BASE",
        Provider::Anthropic => "SBWB_AI_ANTHROPIC_BASE",
        Provider::Google => "SBWB_AI_GOOGLE_BASE",
    };
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            match provider {
                Provider::OpenAi => OPENAI,
                Provider::Anthropic => ANTHROPIC,
                Provider::Google => GOOGLE,
            }
            .to_string()
        })
}

fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        .user_agent(concat!("sbwb/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AiError::Connection(e.to_string()))
}

fn send(req: reqwest::blocking::RequestBuilder, what: &str) -> Result<Value> {
    let resp = req.send().map_err(|e| {
        if e.is_timeout() {
            AiError::Connection(format!("{what}: timed out (no retry was made)"))
        } else if e.is_connect() {
            AiError::Connection(format!("{what}: could not connect ({e})"))
        } else {
            AiError::Connection(format!("{what}: {e}"))
        }
    })?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    let detail = body
        .pointer("/error/message")
        .or_else(|| body.pointer("/error"))
        .map(|m| {
            m.as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| m.to_string())
        })
        .unwrap_or_default();
    match status.as_u16() {
        200..=299 => Ok(body),
        401 | 403 => Err(AiError::Auth(format!(
            "the key was rejected ({}){}",
            status.as_u16(),
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ))),
        404 if what.contains("models") => Err(AiError::Discovery(format!(
            "model listing not available: {detail}"
        ))),
        400 | 404 | 422 => Err(AiError::Compatibility(format!(
            "{what} refused ({}): {detail}",
            status.as_u16()
        ))),
        429 => Err(AiError::Compatibility(format!(
            "rate limited or out of quota ({}); nothing was retried",
            status.as_u16()
        ))),
        _ => Err(AiError::Connection(format!(
            "{what}: HTTP {}: {detail}",
            status.as_u16()
        ))),
    }
}

/// List the models the key can use (AI-01). A failure here is a discovery
/// error and does not mean the key could not be saved.
pub fn list_models(provider: Provider, api_key: &str, timeout: Duration) -> Result<Vec<ModelInfo>> {
    let c = client(timeout)?;
    let v = match provider {
        Provider::OpenAi => send(
            c.get(format!("{}/models", base(Provider::OpenAi)))
                .bearer_auth(api_key),
            "OpenAI models",
        )?,
        Provider::Anthropic => send(
            c.get(format!("{}/models", base(Provider::Anthropic)))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01"),
            "Anthropic models",
        )?,
        Provider::Google => send(
            c.get(format!("{}/models", base(Provider::Google)))
                .query(&[("key", api_key), ("pageSize", "200")]),
            "Google models",
        )?,
    };
    let mut out = Vec::new();
    match provider {
        Provider::OpenAi => {
            for m in v
                .get("data")
                .and_then(|d| d.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                    if id.starts_with("gpt") || id.starts_with("o") {
                        out.push(ModelInfo {
                            id: id.into(),
                            label: id.into(),
                        });
                    }
                }
            }
        }
        Provider::Anthropic => {
            for m in v
                .get("data")
                .and_then(|d| d.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                    let name = m.get("display_name").and_then(|d| d.as_str()).unwrap_or(id);
                    out.push(ModelInfo {
                        id: id.into(),
                        label: name.into(),
                    });
                }
            }
        }
        Provider::Google => {
            for m in v
                .get("models")
                .and_then(|d| d.as_array())
                .into_iter()
                .flatten()
            {
                let supports = m
                    .get("supportedGenerationMethods")
                    .and_then(|s| s.as_array())
                    .map(|a| a.iter().any(|x| x.as_str() == Some("generateContent")))
                    .unwrap_or(false);
                if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                    if supports {
                        let id = name.strip_prefix("models/").unwrap_or(name);
                        let label = m.get("displayName").and_then(|d| d.as_str()).unwrap_or(id);
                        out.push(ModelInfo {
                            id: id.into(),
                            label: label.into(),
                        });
                    }
                }
            }
        }
    }
    if out.is_empty() {
        return Err(AiError::Discovery(
            "the provider returned no usable models; enter a model id by hand".into(),
        ));
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Whether a model takes `temperature: 0`. Newer models refuse sampling
/// settings: Claude Opus 4.7 and later, Sonnet 5 and later, Haiku 5.5,
/// Fable and Mythos return a 400 error, OpenAI's reasoning models (the o
/// series, gpt-5 and later) accept only the default, and Google advises
/// against lowering it on Gemini 3 and later. The older models that take
/// it are a closed list, so every other id, known or not, gets the
/// provider's default.
pub fn takes_temperature(provider: Provider, model: &str) -> bool {
    let m = model.trim();
    let m = m.strip_prefix("models/").unwrap_or(m);
    match provider {
        Provider::OpenAi => m.starts_with("gpt-4") || m.starts_with("gpt-3.5"),
        Provider::Google => m.starts_with("gemini-1.") || m.starts_with("gemini-2."),
        Provider::Anthropic => {
            // the minor version, or a snapshot date for 4.0, follows the family
            let claude4 = |family: &str, minors: &[&str]| {
                m.strip_prefix(family)
                    .and_then(|r| r.strip_prefix('-'))
                    .and_then(|r| r.split('-').next())
                    .is_some_and(|v| {
                        minors.contains(&v)
                            || (v.len() == 8 && v.bytes().all(|b| b.is_ascii_digit()))
                    })
            };
            m.starts_with("claude-3")
                || claude4("claude-opus-4", &["0", "1", "5", "6"])
                || claude4("claude-sonnet-4", &["0", "5", "6"])
                || claude4("claude-haiku-4", &["5"])
        }
    }
}

fn openai_body(conn: &Connection, system: &str, user: &str) -> Value {
    let mut body = json!({
        "model": conn.model,
        "response_format": { "type": "json_object" },
        "messages": [ { "role": "system", "content": system }, { "role": "user", "content": user } ],
    });
    if takes_temperature(conn.provider, &conn.model) {
        body["temperature"] = json!(0);
    }
    body
}

fn anthropic_body(conn: &Connection, system: &str, user: &str) -> Value {
    let legacy = takes_temperature(conn.provider, &conn.model);
    let mut body = json!({
        "model": conn.model,
        // newer models think by default and thinking counts toward this
        // cap, so leave room for it; some older ones allow only 4096
        "max_tokens": if legacy { 4096 } else { 16000 },
        "system": system,
        "messages": [ { "role": "user", "content": user } ],
    });
    if legacy {
        body["temperature"] = json!(0);
    }
    body
}

fn google_body(conn: &Connection, system: &str, user: &str) -> Value {
    let mut config = json!({ "responseMimeType": "application/json" });
    if takes_temperature(conn.provider, &conn.model) {
        config["temperature"] = json!(0);
    }
    json!({
        "systemInstruction": { "parts": [ { "text": system } ] },
        "contents": [ { "role": "user", "parts": [ { "text": user } ] } ],
        "generationConfig": config,
    })
}

/// One completion (AI-02: exactly one request, no retries).
pub fn complete(
    conn: &Connection,
    system: &str,
    user: &str,
    timeout: Duration,
) -> Result<RawReply> {
    let c = client(timeout)?;
    match conn.provider {
        Provider::OpenAi => {
            let body = openai_body(conn, system, user);
            let v = send(
                c.post(format!("{}/chat/completions", base(Provider::OpenAi)))
                    .bearer_auth(&conn.api_key)
                    .json(&body),
                "OpenAI request",
            )?;
            let text = v
                .pointer("/choices/0/message/content")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RawReply {
                text,
                input_tokens: v.pointer("/usage/prompt_tokens").and_then(|t| t.as_u64()),
                output_tokens: v
                    .pointer("/usage/completion_tokens")
                    .and_then(|t| t.as_u64()),
            })
        }
        Provider::Anthropic => {
            let body = anthropic_body(conn, system, user);
            let v = send(
                c.post(format!("{}/messages", base(Provider::Anthropic)))
                    .header("x-api-key", &conn.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&body),
                "Anthropic request",
            )?;
            match v.get("stop_reason").and_then(|s| s.as_str()) {
                Some("refusal") => {
                    return Err(AiError::Validation(
                        "the model declined to answer this page".into(),
                    ))
                }
                Some("max_tokens") => {
                    return Err(AiError::Validation(
                        "the reply reached the output limit before it finished".into(),
                    ))
                }
                _ => {}
            }
            let text = v
                .get("content")
                .and_then(|c| c.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            Ok(RawReply {
                text,
                input_tokens: v.pointer("/usage/input_tokens").and_then(|t| t.as_u64()),
                output_tokens: v.pointer("/usage/output_tokens").and_then(|t| t.as_u64()),
            })
        }
        Provider::Google => {
            let body = google_body(conn, system, user);
            let v = send(
                c.post(format!(
                    "{}/models/{}:generateContent",
                    base(Provider::Google),
                    conn.model
                ))
                .query(&[("key", conn.api_key.as_str())])
                .json(&body),
                "Google request",
            )?;
            let text = v
                .pointer("/candidates/0/content/parts/0/text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RawReply {
                text,
                input_tokens: v
                    .pointer("/usageMetadata/promptTokenCount")
                    .and_then(|t| t.as_u64()),
                output_tokens: v
                    .pointer("/usageMetadata/candidatesTokenCount")
                    .and_then(|t| t.as_u64()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(provider: Provider, model: &str) -> Connection {
        Connection {
            provider,
            model: model.into(),
            api_key: "k".into(),
        }
    }

    #[test]
    fn temperature_only_for_models_that_take_it() {
        use Provider::*;
        for m in [
            "claude-3-5-haiku-20241022",
            "claude-opus-4-20250514",
            "claude-opus-4-1-20250805",
            "claude-opus-4-6",
            "claude-sonnet-4-5-20250929",
            "claude-sonnet-4-6",
            "claude-haiku-4-5-20251001",
        ] {
            assert!(takes_temperature(Anthropic, m), "{m}");
        }
        for m in [
            "claude-sonnet-5",
            "claude-sonnet-5-5",
            "claude-opus-4-7",
            "claude-opus-4-8",
            "claude-opus-5",
            "claude-haiku-5-5",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-something-new",
        ] {
            assert!(!takes_temperature(Anthropic, m), "{m}");
        }
        assert!(takes_temperature(OpenAi, "gpt-4o-mini"));
        assert!(takes_temperature(OpenAi, "gpt-4.1"));
        assert!(!takes_temperature(OpenAi, "gpt-5.4-mini"));
        assert!(!takes_temperature(OpenAi, "gpt-6-astra"));
        assert!(!takes_temperature(OpenAi, "o3"));
        assert!(takes_temperature(Google, "gemini-2.5-flash"));
        assert!(!takes_temperature(Google, "gemini-3.1-pro-preview"));
        assert!(!takes_temperature(Google, "models/gemini-4-argon"));
    }

    #[test]
    fn request_bodies_leave_out_temperature_for_newer_models() {
        let b = anthropic_body(&conn(Provider::Anthropic, "claude-sonnet-5"), "s", "u");
        assert!(b.get("temperature").is_none());
        assert_eq!(b["max_tokens"], 16000);
        let b = anthropic_body(&conn(Provider::Anthropic, "claude-sonnet-4-5"), "s", "u");
        assert_eq!(b["temperature"], 0);
        assert_eq!(b["max_tokens"], 4096);
        let b = openai_body(&conn(Provider::OpenAi, "gpt-5.4-mini"), "s", "u");
        assert!(b.get("temperature").is_none());
        assert_eq!(
            openai_body(&conn(Provider::OpenAi, "gpt-4o"), "s", "u")["temperature"],
            0
        );
        let b = google_body(&conn(Provider::Google, "gemini-3-flash-preview"), "s", "u");
        assert!(b["generationConfig"].get("temperature").is_none());
        assert_eq!(
            b["generationConfig"]["responseMimeType"],
            "application/json"
        );
        let b = google_body(&conn(Provider::Google, "gemini-2.5-flash"), "s", "u");
        assert_eq!(b["generationConfig"]["temperature"], 0);
    }
}
