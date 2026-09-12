//! Scheduler: coordinator thread plus a pool of worker-owning threads.

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
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Status {
    pub state: PipelineState,
    pub stages: Vec<StageProgress>,
    pub activity: String,
    pub workers: u32,
}

struct Unit {
    stage: Stage,
    page: PageIndex,
}

struct UnitResult {
    stage: Stage,
    page: PageIndex,
    result: Result<UnitOk>,
    elapsed_ms: u64,
    worker_died: bool,
}

struct UnitOk {
    output: sbwb_ocr::OcrOutput,
    page_w: f64,
    page_h: f64,
    prep_warnings: Vec<String>,
    render: Option<PathBuf>,
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
    /// Start processing every queued page in the project's scope. Returns
    /// immediately; progress arrives on [`Scheduler::events`].
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
pub fn requeue_for_settings(project: &Project, settings: &ProcessingSettings) -> Result<u32> {
    let fp = settings.ocr_fingerprint();
    let mut n = 0;
    for p in project.pages()? {
        if p.status != PageStatus::Done || p.approved_revision.is_some() {
            continue;
        }
        let runs = project.runs_for_page(p.index)?;
        let last_ocr = runs
            .iter()
            .rev()
            .find(|r| r.stage == Stage::Ocr && r.status == "ok");
        let same = last_ocr
            .and_then(|r| {
                r.settings
                    .get("fingerprint")
                    .and_then(|v| v.as_str())
                    .map(|s| s == fp)
            })
            .unwrap_or(false);
        if !same {
            project.set_page_status(p.index, PageStatus::Queued, None)?;
            n += 1;
        }
    }
    Ok(n)
}

fn coordinate(
    config: SchedulerConfig,
    workers: u32,
    shared: Arc<Shared>,
    tx: Sender<PipelineEvent>,
) -> Result<()> {
    let mut project = Project::open(&config.project_path, OpenMode::ReadWrite)?;
    let meta = project.meta()?;
    let queued: Vec<PageIndex> = project
        .pages()?
        .into_iter()
        .filter(|p| p.status == PageStatus::Queued && meta.scope.contains(p.index))
        .map(|p| p.index)
        .collect();
    let total = queued.len() as u32;
    log(
        &tx,
        "info",
        format!(
            "ocr start · {total} pages · {} · {} dpi · deskew {} · {workers} workers",
            config.settings.model.label(),
            config.settings.dpi,
            if config.settings.deskew { "on" } else { "off" }
        ),
    );
    let started = Instant::now();
    let mut progress = StageProgress {
        stage: Stage::Ocr,
        total,
        done: 0,
        failed: 0,
        running: 0,
        elapsed_ms: 0,
        eta_ms: None,
        secs_per_unit: None,
    };
    publish(&shared, &tx, &progress, "starting");

    // Rendezvous channel: a unit is handed over only when a worker is free,
    // so pausing stops dispatch at the next page boundary.
    let (unit_tx, unit_rx) = bounded::<Unit>(0);
    let (res_tx, res_rx) = unbounded::<UnitResult>();
    let mut pool = Vec::new();
    for i in 0..workers {
        let rx = unit_rx.clone();
        let rtx = res_tx.clone();
        let cfg = config.clone();
        pool.push(
            std::thread::Builder::new()
                .name(format!("pipeline-worker-{i}"))
                .spawn(move || worker_loop(i, cfg, rx, rtx))
                .map_err(SbwbError::other)?,
        );
    }
    drop(res_tx);

    let mut pending = queued.into_iter();
    let mut in_flight = 0u32;
    let mut cancelled = false;
    let mut unit_tx = Some(unit_tx);
    let mut next: Option<Unit> = None;

    loop {
        // Control handling at page boundaries.
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
                if shared
                    .status
                    .lock()
                    .map(|s| s.state)
                    .unwrap_or(PipelineState::Idle)
                    != PipelineState::Paused
                {
                    set_state(&shared, &tx, PipelineState::Paused);
                    log(&tx, "info", "paused".into());
                }
            }
            _ => {
                if shared
                    .status
                    .lock()
                    .map(|s| s.state)
                    .unwrap_or(PipelineState::Idle)
                    == PipelineState::Paused
                {
                    set_state(&shared, &tx, PipelineState::Running);
                    log(&tx, "info", "resumed".into());
                }
            }
        }
        let paused = shared.control.load(Ordering::SeqCst) == CTRL_PAUSE;

        if next.is_none() && !cancelled && !paused {
            next = pending.next().map(|page| Unit {
                stage: Stage::Ocr,
                page,
            });
        }
        if next.is_none() && in_flight == 0 && (cancelled || pending.len() == 0) {
            break;
        }

        // Dispatch when possible, otherwise collect a result.
        let dispatch_ready = next.is_some() && !paused && !cancelled && unit_tx.is_some();
        if dispatch_ready {
            let unit = next.take().unwrap();
            let page = unit.page;
            let sent = crossbeam_channel::select! {
                send(unit_tx.as_ref().unwrap(), unit) -> r => r.is_ok(),
                recv(res_rx) -> r => {
                    if let Ok(res) = r { handle_result(&mut project, &config, &shared, &tx, &mut progress, started, res, &mut in_flight)?; }
                    false
                }
                default(Duration::from_millis(250)) => false,
            };
            if sent {
                project.set_page_status(page, PageStatus::Running, None)?;
                in_flight += 1;
                progress.running = in_flight;
                publish(&shared, &tx, &progress, &format!("ocr {page}"));
            } else if next.is_none() {
                // not sent: put the unit back
                next = Some(Unit {
                    stage: Stage::Ocr,
                    page,
                });
            }
        } else {
            match res_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(res) => handle_result(
                    &mut project,
                    &config,
                    &shared,
                    &tx,
                    &mut progress,
                    started,
                    res,
                    &mut in_flight,
                )?,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    progress.elapsed_ms = started.elapsed().as_millis() as u64;
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
    // Pages still queued after a cancel stay queued (resume later, PIPE-02).
    progress.running = 0;
    progress.elapsed_ms = started.elapsed().as_millis() as u64;
    publish(
        &shared,
        &tx,
        &progress,
        if cancelled { "cancelled" } else { "done" },
    );
    log(
        &tx,
        "info",
        format!(
            "ocr {} · {} done · {} failed · {:.1}s",
            if cancelled { "cancelled" } else { "finished" },
            progress.done,
            progress.failed,
            started.elapsed().as_secs_f64()
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
        done: progress.done,
        failed: progress.failed,
        cancelled,
    });
    project.close()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_result(
    project: &mut Project,
    config: &SchedulerConfig,
    shared: &Shared,
    tx: &Sender<PipelineEvent>,
    progress: &mut StageProgress,
    started: Instant,
    res: UnitResult,
    in_flight: &mut u32,
) -> Result<()> {
    *in_flight = in_flight.saturating_sub(1);
    let page = res.page;
    let mut settings_json = serde_json::to_value(&config.settings)?;
    settings_json["fingerprint"] = serde_json::Value::String(config.settings.ocr_fingerprint());
    let run = project.start_run(
        res.stage,
        Some(page),
        Some("tesseract"),
        Some(config.settings.model.subdir()),
        &settings_json,
    )?;
    match res.result {
        Ok(ok) => {
            project.put_page_ocr(page, run, &ok.output)?;
            project.set_page_size(page, ok.page_w, ok.page_h)?;
            let mut status = serde_json::json!({});
            if let Some(r) = &ok.render {
                status["render"] = serde_json::Value::String(r.to_string_lossy().to_string());
            }
            project.finish_run(run, "ok", None, Some(res.elapsed_ms))?;
            project.set_page_status(page, PageStatus::Done, None)?;
            progress.done += 1;
            let words = ok
                .output
                .words
                .iter()
                .filter(|w| !w.text.is_empty())
                .count() as u32;
            log(
                tx,
                "info",
                format!(
                    "ocr {page} ok · {words} words · mean {:.0} · {:.1}s",
                    ok.output.mean_confidence,
                    res.elapsed_ms as f64 / 1000.0
                ),
            );
            for w in &ok.prep_warnings {
                log(tx, "warn", format!("{page} {w}"));
            }
            let _ = tx.send(PipelineEvent::Unit {
                stage: res.stage,
                page,
                ok: true,
                elapsed_ms: res.elapsed_ms,
                words: Some(words),
                mean_confidence: Some(ok.output.mean_confidence),
                error: None,
                warnings: ok.prep_warnings,
            });
        }
        Err(e) => {
            project.finish_run(run, "failed", Some(&e.to_string()), Some(res.elapsed_ms))?;
            project.set_page_status(page, PageStatus::Failed, Some(&e.to_string()))?;
            progress.failed += 1;
            log(tx, "error", format!("ocr {page} failed · {e}"));
            if res.worker_died {
                log(tx, "warn", "worker replaced after failure".into());
            }
            let _ = tx.send(PipelineEvent::Unit {
                stage: res.stage,
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
    progress.running = *in_flight;
    progress.elapsed_ms = started.elapsed().as_millis() as u64;
    let finished = progress.done + progress.failed;
    if finished >= 3 {
        let per = started.elapsed().as_secs_f64() / finished as f64;
        progress.secs_per_unit = Some(per);
        let remaining = progress.total.saturating_sub(finished);
        progress.eta_ms = Some((per * remaining as f64 * 1000.0) as u64);
    }
    publish(shared, tx, progress, &format!("ocr {page} done"));
    Ok(())
}

fn publish(shared: &Shared, tx: &Sender<PipelineEvent>, progress: &StageProgress, activity: &str) {
    if let Ok(mut s) = shared.status.lock() {
        s.stages = vec![progress.clone()];
        s.activity = activity.to_string();
    }
    let _ = tx.send(PipelineEvent::Progress {
        stages: vec![progress.clone()],
        activity: activity.to_string(),
    });
}

fn worker_loop(index: u32, config: SchedulerConfig, rx: Receiver<Unit>, tx: Sender<UnitResult>) {
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
        let save_render = config
            .keep_renders
            .then(|| renders_dir.join(format!("p{:05}.png", unit.page.0)));
        let result = w
            .call(
                RequestKind::OcrPage {
                    path: config.source_path.clone(),
                    password: None,
                    page: unit.page,
                    dpi: config.settings.dpi,
                    settings: config.settings.ocr(),
                    prep: config.settings.prep(),
                    save_render,
                },
                |_| {},
            )
            .and_then(|resp| match resp {
                Response::Ocr {
                    output,
                    page_w_pt,
                    page_h_pt,
                    render,
                    prep,
                    ..
                } => Ok(UnitOk {
                    output,
                    page_w: page_w_pt,
                    page_h: page_h_pt,
                    prep_warnings: prep.warnings,
                    render,
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
        let _ = index;
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
            PageScope::from_ranges(vec![(59, 58 + pages)], 110),
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
                pdfium_dir: Some(sbwb_pdf_dir()),
                tessdata_dir: Some(sbwb_ocr::default_tessdata_root()),
            },
            keep_renders: false,
        }
    }

    fn sbwb_pdf_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/bin")
    }

    #[test]
    fn runs_two_pages_and_records_evidence() {
        let Some(exe) = worker_exe() else {
            eprintln!("skipping: SBWB_TEST_WORKER_EXE not set");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, src) = make_project(dir.path(), 2);
        let s = Scheduler::start(cfg(&path, &src, exe, Duration::from_secs(120))).unwrap();
        let mut units = 0;
        for ev in s.events().iter() {
            match ev {
                PipelineEvent::Unit { ok, words, .. } => {
                    assert!(ok);
                    assert!(words.unwrap() > 50);
                    units += 1;
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
        assert_eq!(units, 2);
        s.join();
        let p = Project::open(&path, OpenMode::ReadWrite).unwrap();
        assert_eq!(p.counts().unwrap().done, 2);
        let (_, words, _, conf) = p.current_page_ocr(PageIndex(58)).unwrap().unwrap();
        assert!(conf > 50.0);
        assert!(words
            .iter()
            .any(|w| w.text.contains("India") || w.text.contains("the")));
        let runs = p.runs_for_page(PageIndex(58)).unwrap();
        assert_eq!(
            runs[0].settings["fingerprint"].as_str().unwrap(),
            ProcessingSettings {
                model: sbwb_ocr::ModelPack::EngFast,
                ..Default::default()
            }
            .ocr_fingerprint()
        );
        // settings unchanged → nothing to requeue; a model change → requeue
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
                PipelineEvent::Unit { .. } if !first_done => {
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
        assert_eq!(c.done + c.queued, 6);
    }

    #[test]
    fn timeout_fails_the_page_and_continues() {
        let Some(exe) = worker_exe() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, src) = make_project(dir.path(), 2);
        // 1 ms timeout: every page times out, the worker is killed and replaced.
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
