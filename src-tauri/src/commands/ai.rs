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
    /// Where the current run is, for a view opened after it started.
    pub progress: Mutex<Option<AiProgress>>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AiProgress {
    pub page: u32,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub label: &'static str,
    pub key_help: &'static str,
    pub billing_label: &'static str,
    pub billing_url: &'static str,
    pub session_key: bool,
    pub remembered: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiStatus {
    pub providers: Vec<ProviderStatus>,
    pub open_suggestions: u32,
    pub last_run: Option<AiRunRecord>,
    /// The run in progress, if any.
    pub running: Option<AiProgress>,
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
            billing_label: p.billing_label(),
            billing_url: p.billing_url(),
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
    let running = *slot
        .progress
        .lock()
        .map_err(|_| CommandError::new("other", "ai mutex poisoned"))?;
    Ok(AiStatus {
        providers,
        open_suggestions,
        last_run,
        running,
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
    /// Expected reply tokens, for the cost estimate.
    pub approx_output_tokens: usize,
    pub requests: u32,
    /// What leaves this machine, for the consent card (AI-02).
    pub payload_kinds: Vec<&'static str>,
}

/// The estimated bill for the chosen model; `usd` is `None` when SBWB has
/// no price for it (an unknown or custom model id). Separate from the
/// estimate so changing the model does not re-read the pages.
#[derive(Debug, Clone, Serialize)]
pub struct CostEstimate {
    pub usd: Option<f64>,
    pub input_per_mtok: Option<f64>,
    pub output_per_mtok: Option<f64>,
    pub prices_as_of: &'static str,
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

/// The instructions to send: `None` (the default) when blank; an error
/// when longer than the prompt allows.
fn custom_instructions(instructions: Option<&str>) -> CmdResult<Option<&str>> {
    let Some(i) = instructions.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if i.chars().count() > sbwb_ai::prompt::MAX_INSTRUCTIONS {
        return Err(CommandError::new(
            "invalid_input",
            format!(
                "the prompt instructions are longer than {} characters",
                sbwb_ai::prompt::MAX_INSTRUCTIONS
            ),
        ));
    }
    Ok(Some(i))
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptPreview {
    pub default_instructions: &'static str,
    /// Always sent after the instructions; not editable (AI-03).
    pub rules: &'static str,
    pub max_instructions: usize,
    /// The full system prompt for `instructions`.
    pub system: String,
    /// The data message for the requested page, exactly as it would be
    /// sent; `None` when the page has no saved words.
    pub user: Option<String>,
}

/// Show the prompt in the AI tab: the editable instructions, the fixed
/// rules, and the message one page would send.
#[tauri::command]
pub fn ai_prompt(
    state: State<'_, AppState>,
    page: Option<u32>,
    instructions: Option<String>,
) -> CmdResult<PromptPreview> {
    let system = sbwb_ai::prompt::system(custom_instructions(instructions.as_deref())?);
    let user = match page {
        Some(i) => {
            let guard = state
                .project
                .lock()
                .map_err(|_| CommandError::new("other", "state poisoned"))?;
            match guard.as_ref() {
                Some(p) => {
                    let (payload, _) = payload_for(&p.store, PageIndex(i))?;
                    (!payload.words.is_empty()).then(|| sbwb_ai::prompt::user_message(&payload))
                }
                None => None,
            }
        }
        None => None,
    };
    Ok(PromptPreview {
        default_instructions: sbwb_ai::prompt::INSTRUCTIONS,
        rules: sbwb_ai::prompt::RULES,
        max_instructions: sbwb_ai::prompt::MAX_INSTRUCTIONS,
        system,
        user,
    })
}

#[tauri::command]
pub fn ai_estimate(
    state: State<'_, AppState>,
    pages: Vec<u32>,
    instructions: Option<String>,
) -> CmdResult<Estimate> {
    // an over-long prompt is refused at send time; estimate it as written
    let system = sbwb_ai::prompt::system(instructions.as_deref());
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
    let mut output_tokens = 0;
    let mut included = Vec::new();
    for i in pages {
        let (payload, _) = payload_for(&p.store, PageIndex(i))?;
        if payload.words.is_empty() {
            continue;
        }
        words += payload.words.len();
        chars += payload.chars();
        tokens += payload.approx_tokens(&system);
        output_tokens += payload.approx_output_tokens();
        included.push(i);
    }
    Ok(Estimate {
        requests: included.len() as u32,
        pages: included,
        words,
        chars,
        approx_tokens: tokens,
        approx_output_tokens: output_tokens,
        payload_kinds: vec![
            "saved words of the selected pages, one per line with an id",
            "the page number of each page",
        ],
    })
}

/// Price an estimate's tokens for one model, from the table in
/// `sbwb_ai::pricing`.
#[tauri::command]
pub fn ai_cost(
    provider: String,
    model: String,
    input_tokens: usize,
    output_tokens: usize,
) -> CmdResult<CostEstimate> {
    let p = parse_provider(&provider)?;
    let today = jiff::Zoned::now().date().to_string();
    let price = sbwb_ai::pricing::price(p, &model, &today);
    Ok(CostEstimate {
        usd: price.map(|pr| pr.cost(input_tokens, output_tokens)),
        input_per_mtok: price.map(|pr| pr.input),
        output_per_mtok: price.map(|pr| pr.output),
        prices_as_of: sbwb_ai::pricing::PRICES_AS_OF,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSpend {
    pub id: String,
    /// `None` when the model has no price or the provider reported no
    /// token counts.
    pub usd: Option<f64>,
}

/// What AI runs on this book cost, from the tokens each provider reported
/// and the price table in `sbwb_ai::pricing`.
#[derive(Debug, Clone, Serialize)]
pub struct AiSpend {
    /// Every run, newest first.
    pub runs: Vec<RunSpend>,
    /// The sum over the runs with a known cost.
    pub total_usd: f64,
    /// Runs left out of the total because their cost is unknown.
    pub unpriced: u32,
    /// Providers the runs used, most recent first, for links to their
    /// billing pages.
    pub providers: Vec<Provider>,
    pub prices_as_of: &'static str,
}

#[tauri::command]
pub fn ai_spend(state: State<'_, AppState>) -> CmdResult<AiSpend> {
    let usage = {
        let guard = state
            .project
            .lock()
            .map_err(|_| CommandError::new("other", "state poisoned"))?;
        match guard.as_ref() {
            Some(p) => p.store.ai_run_usage()?,
            None => vec![],
        }
    };
    let mut providers = Vec::new();
    for p in usage.iter().filter_map(|u| Provider::parse(&u.provider)) {
        if !providers.contains(&p) {
            providers.push(p);
        }
    }
    let runs: Vec<RunSpend> = usage
        .into_iter()
        .map(|u| RunSpend {
            usd: Provider::parse(&u.provider).and_then(|p| {
                // run timestamps are RFC 3339; the date picks the price
                let date = u.ts.get(..10).unwrap_or(&u.ts);
                sbwb_ai::pricing::run_cost(
                    p,
                    &u.model,
                    date,
                    u.requests,
                    u.input_tokens,
                    u.output_tokens,
                )
            }),
            id: u.id,
        })
        .collect();
    Ok(AiSpend {
        total_usd: runs.iter().filter_map(|r| r.usd).sum(),
        unpriced: runs.iter().filter(|r| r.usd.is_none()).count() as u32,
        providers,
        runs,
        prices_as_of: sbwb_ai::pricing::PRICES_AS_OF,
    })
}

/// Open the provider's billing page in the browser. Only the fixed page for
/// a known provider, never a URL from the page.
#[tauri::command]
pub fn ai_billing_open(provider: String) -> CmdResult<()> {
    let url = parse_provider(&provider)?.billing_url();
    #[cfg(windows)]
    std::process::Command::new("explorer")
        .arg(url)
        .spawn()
        .map_err(|e| CommandError::new("other", format!("open browser: {e}")))?;
    #[cfg(not(windows))]
    let _ = url;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiRunRequest {
    pub provider: String,
    pub model: String,
    pub pages: Vec<u32>,
    pub max_requests: u32,
    pub max_input_chars: usize,
    /// Proofreading instructions from the AI tab; blank or missing means
    /// the default. The fixed rules are always added.
    #[serde(default)]
    pub instructions: Option<String>,
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
    let instructions = custom_instructions(req.instructions.as_deref())?;
    let custom_prompt = instructions.is_some();
    let system = sbwb_ai::prompt::system(instructions);
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
        consent: serde_json::json!({ "payload": ["saved words with ids", "page numbers"], "max_requests": req.max_requests, "max_input_chars": req.max_input_chars, "custom_prompt": custom_prompt, "confirmed": true }),
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
                let progress = AiProgress { page: *page, done: i as u32, total };
                if let Ok(mut p) = handle.state::<AiSlot>().progress.lock() {
                    *p = Some(progress);
                }
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
                match sbwb_ai::check_page(&conn, &system, &payload.0, &budget, &cancel) {
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
                            &serde_json::json!({ "run": record.id, "provider": record.provider, "model": record.model, "pages": record.pages, "status": record.status, "custom_prompt": custom_prompt }),
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
                if let Ok(mut p) = slot.progress.lock() {
                    *p = None;
                }
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
