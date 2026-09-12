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
            let body = json!({
                "model": conn.model,
                "temperature": 0,
                "response_format": { "type": "json_object" },
                "messages": [ { "role": "system", "content": system }, { "role": "user", "content": user } ],
            });
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
            let body = json!({
                "model": conn.model,
                "max_tokens": 4096,
                "temperature": 0,
                "system": system,
                "messages": [ { "role": "user", "content": user } ],
            });
            let v = send(
                c.post(format!("{}/messages", base(Provider::Anthropic)))
                    .header("x-api-key", &conn.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&body),
                "Anthropic request",
            )?;
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
            let body = json!({
                "systemInstruction": { "parts": [ { "text": system } ] },
                "contents": [ { "role": "user", "parts": [ { "text": user } ] } ],
                "generationConfig": { "temperature": 0, "responseMimeType": "application/json" },
            });
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
