//! Scheduler: coordinator thread plus a pool of worker-owning threads.
//!
//! Each page flows OCR then Layout (then Text pass in M5). A finished OCR
//! unit queues that page for Layout at the front so pages complete end to
//! end while later pages are still being recognized.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use jiff::Timestamp;
use sbwb_core::{PageIndex, PageStatus, Result, SbwbError, Stage};
use sbwb_store::{OpenMode, Project};
use sbwb_worker::{RequestKind, Response, Worker, WorkerConfig};

use crate::events::{PipelineEvent, PipelineState, StageProgress};
use crate::settings::{effective_workers, ProcessingSettings};

const CTRL_RUN: u8 = 0;
const CTRL_PAUSE: u8 = 1;
const CTRL_CANCEL: u8 = 2;

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub project_path: PathBuf,
    pub source_path: PathBuf,
    pub settings: ProcessingSettings,
    pub worker: WorkerConfig,
    /// Save recognition renders next to the cache as evidence (IMG-01).
    pub keep_renders: bool,
    /// Lexicon files for the text pass (TXT-01).
    pub lexicon: sbwb_text::LexiconPaths,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Status {
    pub state: PipelineState,
    pub stages: Vec<StageProgress>,
    pub activity: String,
    pub workers: u32,
}

type OcrEvidence = (
    Vec<sbwb_ocr::OcrWord>,
    Vec<sbwb_ocr::OcrLine>,
    Vec<sbwb_ocr::OcrBlock>,
);

struct Unit {
    stage: Stage,
    page: PageIndex,
    /// OCR evidence attached to Layout units.
    ocr: Option<OcrEvidence>,
}

enum UnitOk {
    Text {
        stats: sbwb_text::TextStats,
    },
    Ocr {
        output: sbwb_ocr::OcrOutput,
        page_w: f64,
        page_h: f64,
        prep_warnings: Vec<String>,
    },
    Layout {
        regions: Vec<sbwb_layout::Region>,
        report: sbwb_layout::CoverageReport,
        algorithm: String,
    },
}

struct UnitResult {
    stage: Stage,
    page: PageIndex,
    result: Result<UnitOk>,
    elapsed_ms: u64,
    worker_died: bool,
}

struct Shared {
    control: AtomicU8,
    status: Mutex<Status>,
}

pub struct Scheduler {
    shared: Arc<Shared>,
    coordinator: Option<JoinHandle<()>>,
    events: Receiver<PipelineEvent>,
}

impl Scheduler {
    /// Start processing every queued page in the scope. Returns at once;
    /// progress arrives on [`Scheduler::events`].
    pub fn start(config: SchedulerConfig) -> Result<Self> {
        let (ev_tx, ev_rx) = unbounded::<PipelineEvent>();
        let workers = effective_workers(config.settings.workers);
        let shared = Arc::new(Shared {
            control: AtomicU8::new(CTRL_RUN),
            status: Mutex::new(Status {
                state: PipelineState::Running,
                stages: vec![],
                activity: "starting".into(),
                workers,
            }),
        });
        let shared2 = shared.clone();
        let handle = std::thread::Builder::new()
            .name("pipeline-coordinator".into())
            .spawn(move || {
                if let Err(e) = coordinate(config, workers, shared2.clone(), ev_tx.clone()) {
                    let _ = ev_tx.send(PipelineEvent::Log {
                        ts: Timestamp::now().to_string(),
                        level: "error".into(),
                        text: format!("pipeline stopped: {e}"),
                    });
                    set_state(&shared2, &ev_tx, PipelineState::Idle);
                    let _ = ev_tx.send(PipelineEvent::Finished {
                        done: 0,
                        failed: 0,
                        cancelled: true,
                    });
                }
            })
            .map_err(SbwbError::other)?;
        Ok(Self {
            shared,
            coordinator: Some(handle),
            events: ev_rx,
        })
    }

    pub fn events(&self) -> &Receiver<PipelineEvent> {
        &self.events
    }

    pub fn status(&self) -> Status {
        self.shared
            .status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(Status {
                state: PipelineState::Idle,
                stages: vec![],
                activity: String::new(),
                workers: 0,
            })
    }

    pub fn pause(&self) {
        self.shared.control.store(CTRL_PAUSE, Ordering::SeqCst);
    }
    pub fn resume(&self) {
        self.shared.control.store(CTRL_RUN, Ordering::SeqCst);
    }
    pub fn cancel(&self) {
        self.shared.control.store(CTRL_CANCEL, Ordering::SeqCst);
    }

    pub fn is_finished(&self) -> bool {
        self.coordinator
            .as_ref()
            .map(|h| h.is_finished())
            .unwrap_or(true)
    }

    /// Wait for the coordinator to exit (tests and shutdown).
    pub fn join(mut self) {
        if let Some(h) = self.coordinator.take() {
            let _ = h.join();
        }
    }
}

fn set_state(shared: &Shared, tx: &Sender<PipelineEvent>, state: PipelineState) {
    if let Ok(mut s) = shared.status.lock() {
        s.state = state;
    }
    let _ = tx.send(PipelineEvent::State { state });
}

fn current_state(shared: &Shared) -> PipelineState {
    shared
        .status
        .lock()
        .map(|s| s.state)
        .unwrap_or(PipelineState::Idle)
}

fn log(tx: &Sender<PipelineEvent>, level: &str, text: String) {
    tracing::info!(target: "pipeline", "{text}");
    let _ = tx.send(PipelineEvent::Log {
        ts: Timestamp::now().to_string(),
        level: level.into(),
        text,
    });
}

/// Mark pages that finished under different OCR settings as queued again
/// (PIPE-01: a setting change invalidates only dependent results).
/// Which stage a page must redo under `settings`, if any (PIPE-01):
/// OCR when the recognition fingerprint changed, the text pass alone when
/// only the auto-apply threshold changed. Approved pages never rerun.
pub fn stage_to_redo(
    project: &Project,
    page: &sbwb_store::PageRow,
    settings: &ProcessingSettings,
) -> Result<Option<Stage>> {
    if !page.ocr_done || page.approved_revision.is_some() {
        return Ok(None);
    }
    let fp = settings.ocr_fingerprint();
    let runs = project.runs_for_page(page.index)?;
    let last_ok = |stage: Stage| {
        runs.iter()
            .rev()
            .find(|r| r.stage == stage && r.status == "ok")
    };
    let same_ocr = last_ok(Stage::Ocr)
        .and_then(|r| {
            r.settings
                .get("fingerprint")
                .and_then(|v| v.as_str())
                .map(|s| s == fp)
        })
        .unwrap_or(false);
    if !same_ocr {
        return Ok(Some(Stage::Ocr));
    }
    if page.text_done {
        let same_threshold = last_ok(Stage::TextPass)
            .and_then(|r| {
                r.settings
                    .get("auto_apply_threshold")
                    .and_then(|v| v.as_u64())
            })
            .map(|t| t == settings.auto_apply_threshold as u64)
            .unwrap_or(true);
        if !same_threshold {
            return Ok(Some(Stage::TextPass));
        }
    }
    Ok(None)
}

pub fn requeue_for_settings(project: &Project, settings: &ProcessingSettings) -> Result<u32> {
    let mut n = 0;
    for p in project.pages()? {
        match stage_to_redo(project, &p, settings)? {
            Some(Stage::TextPass) => {
                project.set_stage_done(p.index, Stage::TextPass, false)?;
                project.set_page_status(p.index, PageStatus::Queued, None)?;
                n += 1;
            }
            Some(_) => {
                project.set_stage_done(p.index, Stage::Ocr, false)?;
                project.set_stage_done(p.index, Stage::Layout, false)?;
                project.set_stage_done(p.index, Stage::TextPass, false)?;
                project.set_page_status(p.index, PageStatus::Queued, None)?;
                n += 1;
            }
            None => {}
        }
    }
    Ok(n)
}

struct Progress {
    ocr: StageProgress,
    layout: StageProgress,
    text: StageProgress,
    started: Instant,
}

impl Progress {
    fn stages(&self) -> Vec<StageProgress> {
        vec![self.ocr.clone(), self.layout.clone(), self.text.clone()]
    }
    fn for_stage(&mut self, stage: Stage) -> &mut StageProgress {
        match stage {
            Stage::Layout => &mut self.layout,
            Stage::TextPass => &mut self.text,
            _ => &mut self.ocr,
        }
    }
    fn tick(&mut self) {
        let e = self.started.elapsed().as_millis() as u64;
        self.ocr.elapsed_ms = e;
        self.layout.elapsed_ms = e;
        self.text.elapsed_ms = e;
        let finished = self.ocr.done + self.ocr.failed;
        if finished >= 3 {
            let per = self.started.elapsed().as_secs_f64() / finished as f64;
            self.ocr.secs_per_unit = Some(per);
            let remaining = self.ocr.total.saturating_sub(finished);
            self.ocr.eta_ms = Some((per * remaining as f64 * 1000.0) as u64);
        }
    }
}

fn empty_progress(stage: Stage, total: u32) -> StageProgress {
    StageProgress {
        stage,
        total,
        done: 0,
        failed: 0,
        running: 0,
        elapsed_ms: 0,
        eta_ms: None,
        secs_per_unit: None,
    }
}

fn coordinate(
    config: SchedulerConfig,
    workers: u32,
    shared: Arc<Shared>,
    tx: Sender<PipelineEvent>,
) -> Result<()> {
    let mut project = Project::open(&config.project_path, OpenMode::ReadWrite)?;
    let meta = project.meta()?;
    let pages = project.pages()?;
    let mut ocr_queue: VecDeque<PageIndex> = VecDeque::new();
    let mut layout_queue: VecDeque<PageIndex> = VecDeque::new();
    let mut text_queue: VecDeque<PageIndex> = VecDeque::new();
    for p in pages.iter().filter(|p| meta.scope.contains(p.index)) {
        let queued = matches!(p.status, PageStatus::Queued | PageStatus::Running);
        if queued && !p.ocr_done {
            ocr_queue.push_back(p.index);
        } else if p.ocr_done && !p.layout_done && p.status != PageStatus::Failed {
            layout_queue.push_back(p.index);
        } else if p.ocr_done && p.layout_done && !p.text_done && p.status != PageStatus::Failed {
            text_queue.push_back(p.index);
        }
    }
    let ocr_total = ocr_queue.len() as u32;
    let layout_total = ocr_total + layout_queue.len() as u32;
    let text_total = layout_total + text_queue.len() as u32;
    // The text pass is pure Rust and runs on its own thread with one
    // lexicon, so dictionary lookups never hold up OCR dispatch.
    let mut lexicon = match sbwb_text::Lexicon::load(&config.lexicon) {
        Ok(l) => l,
        Err(e) => {
            log(
                &tx,
                "warn",
                format!("lexicon unavailable ({e}); text pass runs without a dictionary"),
            );
            sbwb_text::Lexicon::empty()
        }
    };
    for (word, kind) in project.vocab()? {
        if kind == "protected" {
            lexicon.add_protected([word]);
        } else {
            lexicon.add_vocab([word]);
        }
    }
    let policy = config.settings.auto_apply_policy();
    log(
        &tx,
        "info",
        format!(
            "start · {ocr_total} pages to recognize · {} layouts pending · {} · {} dpi · deskew {} · {workers} workers",
            layout_queue.len(),
            config.settings.model.label(),
            config.settings.dpi,
            if config.settings.deskew { "on" } else { "off" }
        ),
    );
    let mut progress = Progress {
        ocr: empty_progress(Stage::Ocr, ocr_total),
        layout: empty_progress(Stage::Layout, layout_total),
        text: empty_progress(Stage::TextPass, text_total),
        started: Instant::now(),
    };
    publish(&shared, &tx, &progress, "starting");

    // Rendezvous channel: a unit is handed over only when a worker is free,
    // so pausing stops dispatch at the next page boundary.
    let (unit_tx, unit_rx) = bounded::<Unit>(0);
    let (res_tx, res_rx) = unbounded::<UnitResult>();
    let (text_tx, text_rx) = unbounded::<PageIndex>();
    let text_thread = {
        let res_tx = res_tx.clone();
        let path = config.project_path.clone();
        let scope = meta.scope.clone();
        let policy = policy.clone();
        std::thread::Builder::new()
            .name("sbwb-text".into())
            .spawn(move || text_loop(path, lexicon, policy, scope, text_rx, res_tx))
            .map_err(|e| SbwbError::Other(format!("text thread: {e}")))?
    };
    let text_tx = Some(text_tx);
    let mut pool = Vec::new();
    for i in 0..workers {
        let rx = unit_rx.clone();
        let rtx = res_tx.clone();
        let cfg = config.clone();
        pool.push(
            std::thread::Builder::new()
                .name(format!("pipeline-worker-{i}"))
                .spawn(move || worker_loop(cfg, rx, rtx))
                .map_err(SbwbError::other)?,
        );
    }
    drop(res_tx);

    let mut in_flight = 0u32;
    let mut cancelled = false;
    let mut unit_tx = Some(unit_tx);
    let mut next: Option<Unit> = None;

    loop {
        match shared.control.load(Ordering::SeqCst) {
            CTRL_CANCEL => {
                if !cancelled {
                    cancelled = true;
                    unit_tx = None; // workers exit after their current unit
                    set_state(&shared, &tx, PipelineState::Stopping);
                    log(
                        &tx,
                        "info",
                        "cancel requested · finishing running pages".into(),
                    );
                }
            }
            CTRL_PAUSE => {
                if current_state(&shared) != PipelineState::Paused {
                    set_state(&shared, &tx, PipelineState::Paused);
                    log(&tx, "info", "paused".into());
                }
            }
            _ => {
                if current_state(&shared) == PipelineState::Paused {
                    set_state(&shared, &tx, PipelineState::Running);
                    log(&tx, "info", "resumed".into());
                }
            }
        }
        let paused = shared.control.load(Ordering::SeqCst) == CTRL_PAUSE;

        // Text-pass units go to the text thread. A cancel still lets pages
        // whose layout is already stored finish; only a pause holds them.
        while let (Some(page), Some(ttx)) = (text_queue.front().copied(), text_tx.as_ref()) {
            if paused {
                break;
            }
            if ttx.send(page).is_err() {
                break;
            }
            text_queue.pop_front();
            project.set_page_status(page, PageStatus::Running, None)?;
            in_flight += 1;
            progress.text.running += 1;
            publish(&shared, &tx, &progress, &format!("text {page}"));
        }
        if next.is_none() && !cancelled && !paused {
            // Layout units first: they are cheap and complete pages end to end.
            if let Some(page) = layout_queue.pop_front() {
                match project.current_page_ocr(page)? {
                    Some((_, words, lines, _)) => {
                        let blocks = blocks_from_lines(&lines);
                        next = Some(Unit {
                            stage: Stage::Layout,
                            page,
                            ocr: Some((words, lines, blocks)),
                        });
                    }
                    None => {
                        log(
                            &tx,
                            "warn",
                            format!("{page} has no OCR evidence; skipping layout"),
                        );
                        progress.layout.failed += 1;
                    }
                }
            } else if let Some(page) = ocr_queue.pop_front() {
                next = Some(Unit {
                    stage: Stage::Ocr,
                    page,
                    ocr: None,
                });
            }
        }
        let queues_empty = ocr_queue.is_empty() && layout_queue.is_empty() && text_queue.is_empty();
        if next.is_none() && in_flight == 0 && (cancelled || queues_empty) {
            break;
        }

        let dispatch_ready = next.is_some() && !paused && !cancelled && unit_tx.is_some();
        if dispatch_ready {
            let unit = next.take().unwrap();
            let (page, stage) = (unit.page, unit.stage);
            let sent = crossbeam_channel::select! {
                send(unit_tx.as_ref().unwrap(), unit) -> r => r.is_ok(),
                recv(res_rx) -> r => {
                    if let Ok(res) = r {
                        handle_result(&mut project, &config, &shared, &tx, &mut progress, res, &mut in_flight, &mut layout_queue, &mut text_queue)?;
                    }
                    false
                }
                default(Duration::from_millis(250)) => false,
            };
            if sent {
                project.set_page_status(page, PageStatus::Running, None)?;
                in_flight += 1;
                progress.for_stage(stage).running += 1;
                publish(
                    &shared,
                    &tx,
                    &progress,
                    &format!("{} {page}", stage_word(stage)),
                );
            } else if next.is_none() {
                // Not sent: put the page back so it is rebuilt with its evidence.
                match stage {
                    Stage::Layout => layout_queue.push_front(page),
                    _ => ocr_queue.push_front(page),
                }
            }
        } else {
            match res_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(res) => handle_result(
                    &mut project,
                    &config,
                    &shared,
                    &tx,
                    &mut progress,
                    res,
                    &mut in_flight,
                    &mut layout_queue,
                    &mut text_queue,
                )?,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    progress.tick();
                    publish(
                        &shared,
                        &tx,
                        &progress,
                        if paused { "paused" } else { "working" },
                    );
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    drop(unit_tx);
    for h in pool {
        let _ = h.join();
    }
    drop(text_tx);
    let _ = text_thread.join();
    // Pages left mid-way stay queued for a later resume (PIPE-02).
    for p in project.pages()? {
        if p.status == PageStatus::Running {
            project.set_page_status(p.index, PageStatus::Queued, None)?;
        }
    }
    progress.ocr.running = 0;
    progress.layout.running = 0;
    progress.text.running = 0;
    progress.tick();
    publish(
        &shared,
        &tx,
        &progress,
        if cancelled { "cancelled" } else { "done" },
    );
    let failed = progress.ocr.failed + progress.layout.failed;
    log(
        &tx,
        "info",
        format!(
            "{} · {} pages recognized · {} laid out · {} failed · {:.1}s",
            if cancelled { "cancelled" } else { "finished" },
            progress.ocr.done,
            progress.layout.done,
            failed,
            progress.started.elapsed().as_secs_f64()
        ),
    );
    set_state(
        &shared,
        &tx,
        if cancelled {
            PipelineState::Idle
        } else {
            PipelineState::Done
        },
    );
    let _ = tx.send(PipelineEvent::Finished {
        done: progress.ocr.done,
        failed,
        cancelled,
    });
    project.close()?;
    Ok(())
}

fn stage_word(stage: Stage) -> &'static str {
    match stage {
        Stage::Layout => "layout",
        Stage::TextPass => "text",
        _ => "ocr",
    }
}

/// The text thread: its own connection (the writer lock is per process),
/// one lexicon for the run, pages in the order layout finished.
fn text_loop(
    path: PathBuf,
    lexicon: sbwb_text::Lexicon,
    policy: sbwb_text::AutoApplyPolicy,
    scope: sbwb_core::PageScope,
    rx: Receiver<PageIndex>,
    res_tx: Sender<UnitResult>,
) {
    let mut project = match Project::open(&path, OpenMode::ReadWrite) {
        Ok(p) => p,
        Err(e) => {
            // Every page sent here fails with the same cause; the coordinator
            // records it per page.
            for page in rx.iter() {
                let _ = res_tx.send(UnitResult {
                    stage: Stage::TextPass,
                    page,
                    result: Err(SbwbError::Other(format!(
                        "text pass could not open the book: {e}"
                    ))),
                    elapsed_ms: 0,
                    worker_died: false,
                });
            }
            return;
        }
    };
    for page in rx.iter() {
        let started = Instant::now();
        let result = run_text_pass(&mut project, &lexicon, &policy, page, &scope);
        let _ = res_tx.send(UnitResult {
            stage: Stage::TextPass,
            page,
            result: result.map(|stats| UnitOk::Text { stats }),
            elapsed_ms: started.elapsed().as_millis() as u64,
            worker_died: false,
        });
    }
}

/// Run the text pass for one page (TXT-02, TXT-03).
fn run_text_pass(
    project: &mut Project,
    lexicon: &sbwb_text::Lexicon,
    policy: &sbwb_text::AutoApplyPolicy,
    page: PageIndex,
    scope: &sbwb_core::PageScope,
) -> Result<sbwb_text::TextStats> {
    let (run, words, _lines, _) = project
        .current_page_ocr(page)?
        .ok_or_else(|| SbwbError::NotFound(format!("{page} has no OCR evidence")))?;
    let layout = project.page_layout(page)?;
    let regions: Vec<sbwb_layout::Region> = layout
        .map(|l| serde_json::from_value(l.regions).unwrap_or_default())
        .unwrap_or_default();
    let row = project
        .pages()?
        .into_iter()
        .find(|r| r.index == page)
        .ok_or_else(|| SbwbError::NotFound(format!("{page} row")))?;
    // previous page tail for cross-page joins
    let previous_tail = if page.0 > 0 && scope.contains(PageIndex(page.0 - 1)) {
        let prev = PageIndex(page.0 - 1);
        project
            .page_spans(prev)?
            .into_iter()
            .rev()
            .find(|s| s.region.is_some())
            .filter(|s| s.text.ends_with('-') && s.text.chars().count() >= 3)
            .map(|s| sbwb_text::reconstruct::PendingHyphen {
                span: s.id,
                page: prev,
                revision: s.revision,
                text: s.text.clone(),
            })
    } else {
        None
    };
    let input = sbwb_text::PageTextInput {
        page,
        run,
        words: &words,
        regions: &regions,
        page_w: row.width_pt.unwrap_or(612.0),
        page_h: row.height_pt.unwrap_or(792.0),
        previous_tail,
    };
    let out = sbwb_text::reconstruct(&input, lexicon, policy);
    project.put_page_text(page, Some(run), &out.spans, &out.proposals)?;
    // cross-page join: apply now when it qualifies (D-24 rule a)
    for p in project.page_proposals(page)? {
        if p.cross_page && p.status == "open" && p.score >= policy.threshold {
            project.apply_cross_page_join(&p, page)?;
        }
    }
    let applied = out.stats.auto_applied;
    if applied > 0 {
        project.add_history(
            "text_pass",
            &format!("Text pass · page {} · {applied} automatic changes", page.number()),
            &serde_json::json!({ "page": page.0, "applied": applied, "proposals": out.proposals.iter().filter(|p| p.auto_applied).map(|p| serde_json::json!({"id": p.id, "original": p.original, "replacement": p.replacement})).collect::<Vec<_>>() }),
        )?;
    }
    Ok(out.stats)
}

/// Tesseract blocks are not persisted; rebuild them from stored lines (one
/// block per Tesseract block id) for the segmentation comparison.
fn blocks_from_lines(lines: &[sbwb_ocr::OcrLine]) -> Vec<sbwb_ocr::OcrBlock> {
    let mut map: std::collections::BTreeMap<u32, (sbwb_core::Rect, sbwb_core::Rect)> =
        std::collections::BTreeMap::new();
    for l in lines {
        let e = map.entry(l.block).or_insert((l.bbox, l.bbox_px));
        e.0 = e.0.union(&l.bbox);
        e.1 = e.1.union(&l.bbox_px);
    }
    map.into_iter()
        .map(|(block, (bbox, bbox_px))| sbwb_ocr::OcrBlock {
            block,
            bbox,
            bbox_px,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn handle_result(
    project: &mut Project,
    config: &SchedulerConfig,
    shared: &Shared,
    tx: &Sender<PipelineEvent>,
    progress: &mut Progress,
    res: UnitResult,
    in_flight: &mut u32,
    layout_queue: &mut VecDeque<PageIndex>,
    text_queue: &mut VecDeque<PageIndex>,
) -> Result<()> {
    *in_flight = in_flight.saturating_sub(1);
    let page = res.page;
    let stage = res.stage;
    let st = progress.for_stage(stage);
    st.running = st.running.saturating_sub(1);
    let mut settings_json = serde_json::to_value(&config.settings)?;
    settings_json["fingerprint"] = serde_json::Value::String(config.settings.ocr_fingerprint());
    let (engine, model) = match stage {
        Stage::Layout => ("sbwb-layout", "cc-smear"),
        Stage::TextPass => ("sbwb-text", "rules/1"),
        _ => ("tesseract", config.settings.model.subdir()),
    };
    let run = project.start_run(stage, Some(page), Some(engine), Some(model), &settings_json)?;
    match res.result {
        Ok(UnitOk::Ocr {
            output,
            page_w,
            page_h,
            prep_warnings,
        }) => {
            project.put_page_ocr(page, run, &output)?;
            project.set_page_size(page, page_w, page_h)?;
            project.finish_run(run, "ok", None, Some(res.elapsed_ms))?;
            project.set_stage_done(page, Stage::Ocr, true)?;
            project.set_stage_done(page, Stage::Layout, false)?;
            progress.ocr.done += 1;
            let words = output.words.iter().filter(|w| !w.text.is_empty()).count() as u32;
            log(
                tx,
                "info",
                format!(
                    "ocr {page} ok · {words} words · mean {:.0} · {:.1}s",
                    output.mean_confidence,
                    res.elapsed_ms as f64 / 1000.0
                ),
            );
            for w in &prep_warnings {
                log(tx, "warn", format!("{page} {w}"));
            }
            let _ = tx.send(PipelineEvent::Unit {
                stage,
                page,
                ok: true,
                elapsed_ms: res.elapsed_ms,
                words: Some(words),
                mean_confidence: Some(output.mean_confidence),
                error: None,
                warnings: prep_warnings,
            });
            // Layout follows at once for this page.
            layout_queue.push_front(page);
        }
        Ok(UnitOk::Text { stats }) => {
            project.finish_run(run, "ok", None, Some(res.elapsed_ms))?;
            project.set_stage_done(page, Stage::TextPass, true)?;
            project.set_page_status(page, PageStatus::Done, None)?;
            progress.text.done += 1;
            log(
                tx,
                "info",
                format!(
                    "text {page} ok · {} words · {} paragraphs · {} applied · {} suggested",
                    stats.words,
                    stats.paragraphs,
                    stats.auto_applied,
                    stats.proposals.saturating_sub(stats.auto_applied)
                ),
            );
            let _ = tx.send(PipelineEvent::Unit {
                stage,
                page,
                ok: true,
                elapsed_ms: res.elapsed_ms,
                words: Some(stats.words),
                mean_confidence: None,
                error: None,
                warnings: vec![],
            });
        }
        Ok(UnitOk::Layout {
            regions,
            report,
            algorithm,
        }) => {
            let stored = project.put_page_layout(
                page,
                Some(run),
                &serde_json::to_value(&regions)?,
                &serde_json::to_value(&report)?,
                Some(&algorithm),
                false,
                false,
            )?;
            project.finish_run(run, "ok", None, Some(res.elapsed_ms))?;
            project.set_stage_done(page, Stage::Layout, true)?;
            project.set_stage_done(page, Stage::TextPass, false)?;
            progress.layout.done += 1;
            text_queue.push_back(page);
            let kinds: Vec<String> = regions.iter().map(|r| r.kind.label().to_string()).collect();
            log(
                tx,
                "info",
                format!(
                    "layout {page} ok · {} regions ({}) · {} col · {}",
                    regions.len(),
                    summarize(&kinds),
                    report.columns,
                    if stored {
                        "stored"
                    } else {
                        "kept manual layout"
                    }
                ),
            );
            for w in &report.warnings {
                log(tx, "warn", format!("{page} {w}"));
            }
            let _ = tx.send(PipelineEvent::Unit {
                stage,
                page,
                ok: true,
                elapsed_ms: res.elapsed_ms,
                words: None,
                mean_confidence: None,
                error: None,
                warnings: report.warnings.clone(),
            });
        }
        Err(e) => {
            project.finish_run(run, "failed", Some(&e.to_string()), Some(res.elapsed_ms))?;
            project.set_page_status(page, PageStatus::Failed, Some(&e.to_string()))?;
            progress.for_stage(stage).failed += 1;
            log(
                tx,
                "error",
                format!("{} {page} failed · {e}", stage_word(stage)),
            );
            if res.worker_died {
                log(tx, "warn", "worker replaced after failure".into());
            }
            let _ = tx.send(PipelineEvent::Unit {
                stage,
                page,
                ok: false,
                elapsed_ms: res.elapsed_ms,
                words: None,
                mean_confidence: None,
                error: Some(e.to_string()),
                warnings: vec![],
            });
        }
    }
    progress.tick();
    publish(
        shared,
        tx,
        progress,
        &format!("{} {page} done", stage_word(stage)),
    );
    Ok(())
}

fn summarize(kinds: &[String]) -> String {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for k in kinds {
        *counts.entry(k.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(k, n)| {
            if n > 1 {
                format!("{n} {k}")
            } else {
                k.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn publish(shared: &Shared, tx: &Sender<PipelineEvent>, progress: &Progress, activity: &str) {
    if let Ok(mut s) = shared.status.lock() {
        s.stages = progress.stages();
        s.activity = activity.to_string();
    }
    let _ = tx.send(PipelineEvent::Progress {
        stages: progress.stages(),
        activity: activity.to_string(),
    });
}

fn worker_loop(config: SchedulerConfig, rx: Receiver<Unit>, tx: Sender<UnitResult>) {
    let mut worker: Option<Worker> = None;
    let renders_dir = sbwb_store::cache_dir_for(&config.project_path).join("ocr-renders");
    while let Ok(unit) = rx.recv() {
        let started = Instant::now();
        let mut died = false;
        if worker.as_mut().map(|w| !w.is_alive()).unwrap_or(true) {
            match Worker::spawn(config.worker.clone()) {
                Ok(w) => worker = Some(w),
                Err(e) => {
                    let _ = tx.send(UnitResult {
                        stage: unit.stage,
                        page: unit.page,
                        result: Err(e),
                        elapsed_ms: 0,
                        worker_died: true,
                    });
                    continue;
                }
            }
        }
        let w = worker.as_mut().expect("worker");
        let request = match unit.stage {
            Stage::Layout => {
                let (words, lines, blocks) = unit.ocr.unwrap_or_default();
                RequestKind::LayoutPage {
                    path: config.source_path.clone(),
                    password: None,
                    page: unit.page,
                    words,
                    lines,
                    blocks,
                    settings: sbwb_layout::AnalysisSettings::default(),
                }
            }
            _ => RequestKind::OcrPage {
                path: config.source_path.clone(),
                password: None,
                page: unit.page,
                dpi: config.settings.dpi,
                settings: config.settings.ocr(),
                prep: config.settings.prep(),
                save_render: config
                    .keep_renders
                    .then(|| renders_dir.join(format!("p{:05}.png", unit.page.0))),
            },
        };
        let result = w.call(request, |_| {}).and_then(|resp| match resp {
            Response::Ocr {
                output,
                page_w_pt,
                page_h_pt,
                prep,
                ..
            } => Ok(UnitOk::Ocr {
                output,
                page_w: page_w_pt,
                page_h: page_h_pt,
                prep_warnings: prep.warnings,
            }),
            Response::Layout {
                regions,
                report,
                algorithm,
                ..
            } => Ok(UnitOk::Layout {
                regions,
                report,
                algorithm,
            }),
            other => Err(SbwbError::other(format!("unexpected reply {other:?}"))),
        });
        if matches!(result, Err(SbwbError::Timeout(_))) || !w.is_alive() {
            died = true;
            worker = None;
        }
        let _ = tx.send(UnitResult {
            stage: unit.stage,
            page: unit.page,
            result,
            elapsed_ms: started.elapsed().as_millis() as u64,
            worker_died: died,
        });
    }
    if let Some(w) = worker.take() {
        w.shutdown();
    }
}

/// Convenience for the app: everything the scheduler needs from an open project.
pub fn config_for(
    project: &Project,
    source_path: &Path,
    worker: WorkerConfig,
) -> Result<SchedulerConfig> {
    Ok(SchedulerConfig {
        project_path: project.path().to_path_buf(),
        source_path: source_path.to_path_buf(),
        settings: ProcessingSettings::load(project)?,
        worker,
        keep_renders: false,
        lexicon: sbwb_text::lexicon::default_paths(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::PageScope;
    use sbwb_store::SourceInfo;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
    }

    fn worker_exe() -> Option<PathBuf> {
        std::env::var_os("SBWB_TEST_WORKER_EXE").map(PathBuf::from)
    }

    fn make_project(dir: &Path, pages: u32) -> (PathBuf, PathBuf) {
        let bytes = std::fs::read(fixture()).unwrap();
        let info = SourceInfo {
            name: "fixture.pdf".into(),
            size: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            page_count: 110,
            title: None,
            author: None,
        };
        let path = dir.join("book.sbwb");
        let p = Project::create(
            &path,
            &fixture(),
            info,
            &[],
            PageScope::from_ranges(vec![(48, 47 + pages)], 110),
            "test",
        )
        .unwrap();
        let src = p.source_path().unwrap();
        p.close().unwrap();
        (path, src)
    }

    fn cfg(project: &Path, src: &Path, exe: PathBuf, timeout: Duration) -> SchedulerConfig {
        SchedulerConfig {
            project_path: project.to_path_buf(),
            source_path: src.to_path_buf(),
            settings: ProcessingSettings {
                model: sbwb_ocr::ModelPack::EngFast,
                workers: 2,
                ..Default::default()
            },
            worker: WorkerConfig {
                exe: Some(exe),
                timeout,
                grace: Duration::from_millis(300),
                pdfium_dir: Some(pdfium_dir()),
                tessdata_dir: Some(sbwb_ocr::default_tessdata_root()),
            },
            keep_renders: false,
            lexicon: sbwb_text::lexicon::default_paths(),
        }
    }

    fn pdfium_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/bin")
    }

    #[test]
    fn runs_ocr_then_layout_and_records_evidence() {
        let Some(exe) = worker_exe() else {
            eprintln!("skipping: SBWB_TEST_WORKER_EXE not set");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, src) = make_project(dir.path(), 2);
        let s = Scheduler::start(cfg(&path, &src, exe, Duration::from_secs(120))).unwrap();
        let (mut ocr_units, mut layout_units, mut text_units) = (0, 0, 0);
        for ev in s.events().iter() {
            match ev {
                PipelineEvent::Unit {
                    ok, stage, words, ..
                } => {
                    assert!(ok);
                    match stage {
                        Stage::Ocr => {
                            assert!(words.unwrap() > 50);
                            ocr_units += 1;
                        }
                        Stage::Layout => layout_units += 1,
                        Stage::TextPass => text_units += 1,
                        _ => {}
                    }
                }
                PipelineEvent::Finished {
                    done,
                    failed,
                    cancelled,
                } => {
                    assert_eq!((done, failed, cancelled), (2, 0, false));
                    break;
                }
                _ => {}
            }
        }
        assert_eq!((ocr_units, layout_units, text_units), (2, 2, 2));
        s.join();
        let p = Project::open(&path, OpenMode::ReadWrite).unwrap();
        let c = p.counts().unwrap();
        assert_eq!(
            (c.done, c.ocr_done, c.layout_done, c.text_done),
            (2, 2, 2, 2)
        );
        let spans = p.page_spans(PageIndex(47)).unwrap();
        assert!(
            spans.iter().any(|s| s.text == "communication"),
            "joined word missing"
        );
        assert!(p
            .page_proposals(PageIndex(47))
            .unwrap()
            .iter()
            .any(|q| q.status == "applied_auto"));
        let layout = p.page_layout(PageIndex(47)).unwrap().unwrap();
        let regions: Vec<sbwb_layout::Region> = serde_json::from_value(layout.regions).unwrap();
        assert!(regions
            .iter()
            .any(|r| r.kind == sbwb_layout::RegionKind::Body));
        assert!(regions
            .iter()
            .any(|r| r.kind == sbwb_layout::RegionKind::Marginalia));
        let runs = p.runs_for_page(PageIndex(47)).unwrap();
        assert!(runs
            .iter()
            .any(|r| r.stage == Stage::Layout && r.status == "ok"));
        // settings unchanged: nothing to requeue; a model change requeues both
        assert_eq!(
            requeue_for_settings(
                &p,
                &ProcessingSettings {
                    model: sbwb_ocr::ModelPack::EngFast,
                    ..Default::default()
                }
            )
            .unwrap(),
            0
        );
        assert_eq!(
            requeue_for_settings(&p, &ProcessingSettings::default()).unwrap(),
            2
        );
    }

    #[test]
    fn cancel_keeps_finished_pages_and_leaves_the_rest_queued() {
        let Some(exe) = worker_exe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, src) = make_project(dir.path(), 6);
        let mut c = cfg(&path, &src, exe, Duration::from_secs(120));
        c.settings.workers = 1;
        let s = Scheduler::start(c).unwrap();
        let mut first_done = false;
        for ev in s.events().iter() {
            match ev {
                PipelineEvent::Unit {
                    stage: Stage::Layout,
                    ..
                } if !first_done => {
                    first_done = true;
                    s.cancel();
                }
                PipelineEvent::Finished {
                    cancelled, done, ..
                } => {
                    assert!(cancelled);
                    assert!((1..6).contains(&done), "done {done}");
                    break;
                }
                _ => {}
            }
        }
        s.join();
        let p = Project::open(&path, OpenMode::ReadWrite).unwrap();
        let c = p.counts().unwrap();
        assert!(c.done >= 1);
        assert_eq!(c.running, 0);
        assert_eq!(c.done + c.queued, 6, "{c:?}");
    }

    #[test]
    fn timeout_fails_the_page_and_continues() {
        let Some(exe) = worker_exe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, src) = make_project(dir.path(), 2);
        let mut c = cfg(&path, &src, exe, Duration::from_millis(1));
        c.settings.workers = 1;
        let s = Scheduler::start(c).unwrap();
        for ev in s.events().iter() {
            if let PipelineEvent::Finished { done, failed, .. } = ev {
                assert_eq!((done, failed), (0, 2));
                break;
            }
        }
        s.join();
        let p = Project::open(&path, OpenMode::ReadWrite).unwrap();
        let pages = p.pages().unwrap();
        let failed: Vec<_> = pages
            .iter()
            .filter(|p| p.status == PageStatus::Failed)
            .collect();
        assert_eq!(failed.len(), 2);
        assert!(failed[0].error.as_deref().unwrap().contains("timed out"));
    }
}
