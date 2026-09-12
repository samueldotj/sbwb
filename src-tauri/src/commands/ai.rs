//! AI proofreading commands (AI-01..03): provider status, keys, model
//! discovery, the consent estimate, a bounded run on a background thread,
//! cancel, and run history. Nothing here runs without an explicit request.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sbwb_ai::{AiError, Budget, Connection, ModelInfo, PagePayload, Provider};
use sbwb_core::PageIndex;
use sbwb_store::AiRunRecord;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, CmdResult, CommandError};

pub const EVENT_AI: &str = "ai:event";

#[derive(Default)]
pub struct AiSlot {
    pub keys: sbwb_ai::keys::KeyStore,
    pub running: Mutex<bool>,
    pub cancel: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub label: &'static str,
    pub key_help: &'static str,
    pub session_key: bool,
    pub remembered: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiStatus {
    pub providers: Vec<ProviderStatus>,
    pub open_suggestions: u32,
    pub last_run: Option<AiRunRecord>,
}

fn ai_err(e: AiError) -> CommandError {
    let code = match &e {
        AiError::Connection(_) => "ai_connection",
        AiError::Auth(_) => "ai_auth",
        AiError::Discovery(_) => "ai_discovery",
        AiError::Compatibility(_) => "ai_compatibility",
        AiError::Validation(_) => "ai_validation",
        AiError::Budget(_) => "ai_budget",
        AiError::Cancelled => "cancelled",
    };
    CommandError::new(code, e.to_string())
}

fn parse_provider(s: &str) -> CmdResult<Provider> {
    Provider::parse(s)
        .ok_or_else(|| CommandError::new("invalid_input", format!("unknown provider {s}")))
}

#[tauri::command]
pub fn ai_status(app: AppHandle, state: State<'_, AppState>) -> CmdResult<AiStatus> {
    let slot = app.state::<AiSlot>();
    let providers = Provider::all()
        .into_iter()
        .map(|p| ProviderStatus {
            provider: p,
            label: p.label(),
            key_help: p.key_help(),
            session_key: slot.keys.has_session(p),
            remembered: slot.keys.remembered(p),
        })
        .collect();
    let (open_suggestions, last_run) = {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        match guard.as_ref() {
            Some(p) => (
                p.store.ai_open_suggestions()?,
                p.store.ai_runs()?.into_iter().next(),
            ),
            None => (0, None),
        }
    };
    Ok(AiStatus {
        providers,
        open_suggestions,
        last_run,
    })
}

/// Store a key for the session; `remember` also puts it in the OS
/// credential store. The key is never logged.
#[tauri::command]
pub fn ai_key_set(
    app: AppHandle,
    provider: String,
    key: String,
    remember: bool,
) -> CmdResult<bool> {
    let p = parse_provider(&provider)?;
    app.state::<AiSlot>()
        .keys
        .set(p, &key, remember)
        .map_err(|m| CommandError::new("ai_key", m))
}

#[tauri::command]
pub fn ai_key_forget(app: AppHandle, provider: String) -> CmdResult<()> {
    let p = parse_provider(&provider)?;
    app.state::<AiSlot>().keys.forget(p);
    Ok(())
}

/// Model discovery (AI-01). A failure is a discovery error; the key stays.
#[tauri::command]
pub fn ai_models(app: AppHandle, provider: String) -> CmdResult<Vec<ModelInfo>> {
    let p = parse_provider(&provider)?;
    let key = app
        .state::<AiSlot>()
        .keys
        .get(p)
        .ok_or_else(|| CommandError::new("ai_auth", "no key for this provider"))?;
    sbwb_ai::providers::list_models(p, &key, Duration::from_secs(30)).map_err(ai_err)
}

#[derive(Debug, Clone, Serialize)]
pub struct Estimate {
    pub pages: Vec<u32>,
    pub words: usize,
    pub chars: usize,
    pub approx_tokens: usize,
    pub requests: u32,
    /// What leaves this machine, for the consent card (AI-02).
    pub payload_kinds: Vec<&'static str>,
    pub cost: &'static str,
}

fn payload_for(
    store: &sbwb_store::Project,
    page: PageIndex,
) -> sbwb_core::Result<(PagePayload, Vec<(sbwb_core::SpanId, u64)>)> {
    let spans = store.page_spans(page)?;
    let words = spans.iter().map(|s| s.text.clone()).collect();
    let refs = spans.iter().map(|s| (s.id, s.revision)).collect();
    Ok((
        PagePayload {
            page_number: page.number(),
            words,
        },
        refs,
    ))
}

#[tauri::command]
pub fn ai_estimate(state: State<'_, AppState>, pages: Vec<u32>) -> CmdResult<Estimate> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    let p = guard
        .as_ref()
        .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
    let mut words = 0;
    let mut chars = 0;
    let mut tokens = 0;
    let mut included = Vec::new();
    for i in pages {
        let (payload, _) = payload_for(&p.store, PageIndex(i))?;
        if payload.words.is_empty() {
            continue;
        }
        words += payload.words.len();
        chars += payload.chars();
        tokens += payload.approx_tokens();
        included.push(i);
    }
    Ok(Estimate {
        requests: included.len() as u32,
        pages: included,
        words,
        chars,
        approx_tokens: tokens,
        payload_kinds: vec![
            "saved words of the selected pages, one per line with an id",
            "the page number of each page",
        ],
        cost: "unknown (the provider bills by token; SBWB has no price table)",
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiRunRequest {
    pub provider: String,
    pub model: String,
    pub pages: Vec<u32>,
    pub max_requests: u32,
    pub max_input_chars: usize,
    /// The user ticked the consent box on the card that listed the payload.
    pub consent: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiEvent {
    Progress {
        page: u32,
        done: u32,
        total: u32,
    },
    PageDone {
        page: u32,
        suggestions: u32,
        rejected: u32,
    },
    Finished {
        run: AiRunRecord,
    },
    Failed {
        run: AiRunRecord,
        error: CommandError,
    },
}

/// Run the check on the selected pages, one request per page, in order,
/// stopping at the first error (AI-02: no repeated billable requests).
#[tauri::command]
pub fn ai_run(app: AppHandle, state: State<'_, AppState>, req: AiRunRequest) -> CmdResult<String> {
    if !req.consent {
        return Err(CommandError::new(
            "invalid_input",
            "consent is required for every run",
        ));
    }
    let provider = parse_provider(&req.provider)?;
    if req.model.trim().is_empty() {
        return Err(CommandError::new("invalid_input", "choose a model"));
    }
    let slot = app.state::<AiSlot>();
    {
        let mut running = slot
            .running
            .lock()
            .map_err(|_| CommandError::new("other", "ai mutex poisoned"))?;
        if *running {
            return Err(CommandError::new(
                "conflict",
                "an AI run is already in progress",
            ));
        }
        *running = true;
    }
    let key = match slot.keys.get(provider) {
        Some(k) => k,
        None => {
            *slot.running.lock().unwrap() = false;
            return Err(CommandError::new("ai_auth", "no key for this provider"));
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    *slot
        .cancel
        .lock()
        .map_err(|_| CommandError::new("other", "ai mutex poisoned"))? = Some(cancel.clone());
    let run_id = sbwb_core::RunId::new().to_string();
    let pages: Vec<u32> = req.pages.clone();
    if pages.len() as u32 > req.max_requests {
        *slot.running.lock().unwrap() = false;
        return Err(CommandError::new(
            "ai_budget",
            format!(
                "{} pages selected but the request budget is {}",
                pages.len(),
                req.max_requests
            ),
        ));
    }
    let mut record = AiRunRecord {
        id: run_id.clone(),
        ts: jiff::Timestamp::now().to_string(),
        provider: provider.id().into(),
        model: req.model.clone(),
        pages: pages.clone(),
        consent: serde_json::json!({ "payload": ["saved words with ids", "page numbers"], "max_requests": req.max_requests, "max_input_chars": req.max_input_chars, "confirmed": true }),
        requests: 0,
        chars_sent: 0,
        input_tokens: None,
        output_tokens: None,
        suggestions: 0,
        rejected: 0,
        status: "running".into(),
        error: None,
        elapsed_ms: None,
    };
    {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        let p = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("not_found", "no open book"))?;
        p.store.record_ai_run(&record)?;
    }
    let conn = Connection {
        provider,
        model: req.model.clone(),
        api_key: key,
    };
    let budget = Budget {
        max_requests: req.max_requests,
        max_input_chars: req.max_input_chars,
        timeout_secs: 90,
    };
    let handle = app.clone();
    let label = format!("AI · {}/{}", provider.id(), req.model);
    std::thread::Builder::new()
        .name("sbwb-ai".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let total = pages.len() as u32;
            let mut failure: Option<AiError> = None;
            for (i, page) in pages.iter().enumerate() {
                let _ = handle.emit(EVENT_AI, AiEvent::Progress { page: *page, done: i as u32, total });
                // snapshot the page text and span refs under the lock, then release it for the network call
                let payload = {
                    let st = handle.state::<AppState>();
                    let guard = st.project.lock();
                    let got = guard.as_ref().ok().and_then(|g| g.as_ref().map(|p| payload_for(&p.store, PageIndex(*page))));
                    match got {
                        Some(Ok(v)) => v,
                        _ => continue,
                    }
                };
                if payload.0.words.is_empty() {
                    continue;
                }
                match sbwb_ai::check_page(&conn, &payload.0, &budget, &cancel) {
                    Ok(out) => {
                        record.requests += out.usage.requests;
                        record.chars_sent += out.usage.chars_sent as u64;
                        if let Some(t) = out.usage.input_tokens {
                            record.input_tokens = Some(record.input_tokens.unwrap_or(0) + t);
                        }
                        if let Some(t) = out.usage.output_tokens {
                            record.output_tokens = Some(record.output_tokens.unwrap_or(0) + t);
                        }
                        record.rejected += out.rejected.len() as u32;
                        let items: Vec<(sbwb_core::SpanId, u64, String, String, String)> = out
                            .suggestions
                            .iter()
                            .filter_map(|s| payload.1.get(s.index).map(|(id, rev)| (*id, *rev, s.before.clone(), s.after.clone(), s.reason.clone())))
                            .collect();
                        let added = {
                            let st = handle.state::<AppState>();
                            let mut guard = st.project.lock();
                            let got = guard.as_mut().ok().and_then(|g| g.as_mut().map(|p| p.store.merge_ai_suggestions(PageIndex(*page), &label, &items)));
                            match got {
                                Some(Ok(n)) => n,
                                _ => 0,
                            }
                        };
                        record.suggestions += added;
                        let _ = handle.emit(EVENT_AI, AiEvent::PageDone { page: *page, suggestions: added, rejected: out.rejected.len() as u32 });
                        let _ = handle.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
                    }
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
            record.elapsed_ms = Some(started.elapsed().as_millis() as u64);
            record.status = match &failure {
                None => "ok".into(),
                Some(AiError::Cancelled) => "cancelled".into(),
                Some(_) => "failed".into(),
            };
            record.error = failure.as_ref().map(|e| e.to_string());
            {
                let st = handle.state::<AppState>();
                let guard = st.project.lock();
                if let Ok(guard) = guard {
                    if let Some(p) = guard.as_ref() {
                        let _ = p.store.record_ai_run(&record);
                        let _ = p.store.add_history(
                            "ai",
                            &format!("AI check · {} · {} page{} · {} suggestions", label, record.pages.len(), if record.pages.len() == 1 { "" } else { "s" }, record.suggestions),
                            &serde_json::json!({ "run": record.id, "provider": record.provider, "model": record.model, "pages": record.pages, "status": record.status }),
                        );
                    }
                }
            }
            match failure {
                None => {
                    let _ = handle.emit(EVENT_AI, AiEvent::Finished { run: record });
                }
                Some(e) => {
                    let _ = handle.emit(EVENT_AI, AiEvent::Failed { run: record, error: ai_err(e) });
                }
            }
            let _ = handle.emit(crate::commands::project::EVENT_PROJECT_CHANGED, ());
            if let Some(slot) = handle.try_state::<AiSlot>() {
                if let Ok(mut r) = slot.running.lock() {
                    *r = false;
                }
            }
        })
        .map_err(|e| CommandError::new("other", format!("ai thread: {e}")))?;
    Ok(run_id)
}

#[tauri::command]
pub fn ai_cancel(app: AppHandle) -> CmdResult<()> {
    if let Some(c) = app
        .state::<AiSlot>()
        .cancel
        .lock()
        .map_err(|_| CommandError::new("other", "ai mutex poisoned"))?
        .as_ref()
    {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub fn ai_runs(state: State<'_, AppState>) -> CmdResult<Vec<AiRunRecord>> {
    let guard = state
        .project
        .lock()
        .map_err(|_| CommandError::new("other", "state poisoned"))?;
    match guard.as_ref() {
        Some(p) => Ok(p.store.ai_runs()?),
        None => Ok(vec![]),
    }
}
