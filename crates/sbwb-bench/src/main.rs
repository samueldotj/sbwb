//! `sbwb-bench`: the benchmark suite (NFR-02, NFR-03, NFR-05, NFR-07) and
//! the large-book run with resource logging and resume (A-14, NFR-04).
//!
//! Every report identifies the build, OS, CPU, memory, model versions and
//! hashes, input hashes, worker count, warm/cold state, and failures. Use:
//!
//! ```text
//! sbwb-bench                       # throughput (1 vs 4 workers), interaction, export
//! sbwb-bench --pages 30            # pages used for the throughput runs
//! sbwb-bench --full                # the 529-page scan with resource sampling and a resume test
//! sbwb-bench --out docs/benchmarks # where the markdown and json land
//! ```
//!
//! The binary doubles as the worker process (`--worker`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sbwb_core::{PageIndex, PageScope};
use sbwb_pipeline::{PipelineEvent, ProcessingSettings, Scheduler, SchedulerConfig};
use sbwb_store::{OpenMode, Project, SourceInfo};
use sbwb_worker::WorkerConfig;
use serde::Serialize;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sha256_file(p: &Path) -> String {
    use sha2::Digest;
    match std::fs::read(p) {
        Ok(b) => format!("{:x}", sha2::Sha256::digest(&b)),
        Err(_) => "missing".into(),
    }
}

#[derive(Serialize)]
struct Machine {
    host: String,
    os: String,
    cpu: String,
    cores: usize,
    memory_gb: f64,
    build_profile: &'static str,
    git: String,
    app_version: &'static str,
    tesseract: String,
}

fn machine() -> Machine {
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let git = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    Machine {
        host: sysinfo::System::host_name().unwrap_or_default(),
        os: format!(
            "{} {}",
            sysinfo::System::long_os_version().unwrap_or_default(),
            sysinfo::System::cpu_arch()
        ),
        cpu: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default(),
        cores: sys.cpus().len(),
        memory_gb: sys.total_memory() as f64 / 1024f64.powi(3),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        git,
        app_version: env!("CARGO_PKG_VERSION"),
        tesseract: sbwb_ocr::Engine::version(),
    }
}

#[derive(Serialize, Default, Clone)]
struct RunResult {
    label: String,
    pages: u32,
    workers: u32,
    model: String,
    wall_s: f64,
    pages_per_min: f64,
    failed: u32,
    peak_rss_mb: f64,
    ocr_secs_per_page: f64,
}

/// Poll the resident memory of every sbwb process (this one plus workers).
fn spawn_memory_sampler(
    stop: Arc<AtomicBool>,
    peak_kb: Arc<AtomicU64>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut sys = sysinfo::System::new();
        while !stop.load(Ordering::SeqCst) {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            let total: u64 = sys
                .processes()
                .values()
                .filter(|p| p.name().to_string_lossy().starts_with("sbwb"))
                .map(|p| p.memory())
                .sum();
            let kb = total / 1024;
            peak_kb.fetch_max(kb, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(500));
        }
    })
}

fn make_project(dir: &Path, source: &Path, scope: PageScope, pages: u32) -> (PathBuf, PathBuf) {
    let bytes = std::fs::read(source).unwrap();
    let info = SourceInfo {
        name: source.file_name().unwrap().to_string_lossy().into_owned(),
        size: bytes.len() as u64,
        blake3: blake3::hash(&bytes).to_hex().to_string(),
        page_count: pages,
        title: Some("bench".into()),
        author: None,
    };
    let path = dir.join("bench.sbwb");
    let p = Project::create(&path, source, info, &[], scope, "bench").unwrap();
    let src = p.source_path().unwrap();
    p.close().unwrap();
    (path, src)
}

fn config(project: &Path, src: &Path, workers: u32, model: sbwb_ocr::ModelPack) -> SchedulerConfig {
    SchedulerConfig {
        project_path: project.to_path_buf(),
        source_path: src.to_path_buf(),
        settings: ProcessingSettings {
            model,
            workers,
            ..Default::default()
        },
        worker: WorkerConfig {
            exe: Some(std::env::current_exe().unwrap()),
            timeout: Duration::from_secs(120),
            grace: Duration::from_secs(10),
            pdfium_dir: Some(root().join("third_party/pdfium/bin")),
            tessdata_dir: Some(root().join("models")),
        },
        keep_renders: false,
        lexicon: sbwb_text::lexicon::default_paths(),
    }
}

/// Run a scheduler to completion (or until `cancel_at` pages are done),
/// returning wall time, failures, done count, and peak memory.
fn run_pipeline(cfg: SchedulerConfig, cancel_at: Option<u32>) -> (f64, u32, u32, f64) {
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let sampler = spawn_memory_sampler(stop.clone(), peak.clone());
    let t = Instant::now();
    let s = Scheduler::start(cfg).unwrap();
    let mut done_ocr = 0u32;
    let mut cancelled = false;
    let (mut done, mut failed) = (0, 0);
    for ev in s.events().iter() {
        match ev {
            PipelineEvent::Unit {
                stage: sbwb_core::Stage::Ocr,
                ok: true,
                ..
            } => {
                done_ocr += 1;
                if let Some(n) = cancel_at {
                    if done_ocr >= n && !cancelled {
                        s.cancel();
                        cancelled = true;
                    }
                }
            }
            PipelineEvent::Finished {
                done: d, failed: f, ..
            } => {
                done = d;
                failed = f;
                break;
            }
            _ => {}
        }
    }
    let wall = t.elapsed().as_secs_f64();
    s.join();
    stop.store(true, Ordering::SeqCst);
    let _ = sampler.join();
    (
        wall,
        failed,
        done,
        peak.load(Ordering::SeqCst) as f64 / 1024.0,
    )
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

#[derive(Serialize, Default)]
struct Interaction {
    summary_ms_p95: f64,
    next_issue_ms_p95: f64,
    uncached_preview_ms_p95: f64,
    cached_page_ms_p95: f64,
    issues: u32,
}

fn interaction(project: &Path, src: &Path) -> Interaction {
    let p = Project::open(project, OpenMode::ReadWrite).unwrap();
    let mut summary = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        let _ = p.summary().unwrap();
        summary.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let filter = sbwb_review::IssueFilter::default();
    let counts = p.issue_counts(&filter).unwrap();
    let mut next = Vec::new();
    let mut cur = p.next_issue(None, None, true, &filter).unwrap().issue;
    for _ in 0..100 {
        let t = Instant::now();
        cur = p
            .next_issue(cur.map(|i| i.id), None, true, &filter)
            .unwrap()
            .issue;
        next.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    // previews: uncached renders through a worker, then cached file hits
    let mut worker = sbwb_worker::Worker::spawn(WorkerConfig {
        exe: Some(std::env::current_exe().unwrap()),
        pdfium_dir: Some(root().join("third_party/pdfium/bin")),
        tessdata_dir: Some(root().join("models")),
        ..Default::default()
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut uncached = Vec::new();
    let mut cached = Vec::new();
    for i in 0..15u32 {
        let out = dir.path().join(format!("p{i}.webp"));
        let t = Instant::now();
        let _ = worker
            .call(
                sbwb_worker::RequestKind::RenderPage {
                    path: src.to_path_buf(),
                    password: None,
                    page: PageIndex(i),
                    scale: 1.5,
                    crop: None,
                    max_dimension: 8192,
                    out: out.clone(),
                },
                |_| {},
            )
            .unwrap();
        uncached.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        let _ = std::fs::read(&out).unwrap();
        cached.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    worker.shutdown();
    Interaction {
        summary_ms_p95: percentile(&mut summary, 0.95),
        next_issue_ms_p95: percentile(&mut next, 0.95),
        uncached_preview_ms_p95: percentile(&mut uncached, 0.95),
        cached_page_ms_p95: percentile(&mut cached, 0.95),
        issues: counts.unresolved,
    }
}

#[derive(Serialize)]
struct ExportBench {
    words: usize,
    pages: usize,
    wall_s: f64,
    validation_ok: bool,
}

fn export_bench(dir: &Path) -> ExportBench {
    let snap = sbwb_export::test_support::sample_snapshot(500, true);
    let dest = dir.join("bench-250k.docx");
    let t = Instant::now();
    let report = sbwb_export::run(
        &snap,
        &dest,
        &sbwb_export::NoRenders,
        &AtomicBool::new(false),
        &mut |_, _, _| {},
    )
    .unwrap();
    ExportBench {
        words: snap.word_count(),
        pages: snap.pages.len(),
        wall_s: t.elapsed().as_secs_f64(),
        validation_ok: report.validation.ok,
    }
}

#[derive(Serialize)]
struct FullRun {
    source: String,
    source_blake3: String,
    pages: u32,
    workers: u32,
    model: String,
    first_run_wall_s: f64,
    done_at_cancel: u32,
    resume_wall_s: f64,
    done_after_resume: u32,
    failed: u32,
    peak_rss_mb: f64,
    redone_pages: u32,
}

fn full_run(dir: &Path, workers: u32) -> Option<FullRun> {
    let source = root().join("fixtures/corpus/hough-1839-vol1/source-full.pdf");
    if !source.exists() {
        eprintln!("source-full.pdf missing; skipping the full run");
        return None;
    }
    let bytes = std::fs::read(&source).unwrap();
    let info = sbwb_pdf::inspect_file(&source, 3, None).unwrap();
    let pages = info.page_count;
    let (project, src) = make_project(dir, &source, PageScope::all(pages), pages);
    let model = sbwb_ocr::ModelPack::EngBest;
    // first run: interrupt after 60 recognised pages
    let (w1, _f1, _d1, peak1) = run_pipeline(config(&project, &src, workers, model), Some(60));
    let p = Project::open(&project, OpenMode::ReadWrite).unwrap();
    let c = p.counts().unwrap();
    let done_at_cancel = c.done;
    let runs_before: i64 = p
        .conn()
        .query_row(
            "SELECT count(*) FROM runs WHERE stage = 'ocr' AND status = 'ok'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    p.close().unwrap();
    // resume: only the remaining pages run
    let (w2, f2, _d2, peak2) = run_pipeline(config(&project, &src, workers, model), None);
    let p = Project::open(&project, OpenMode::ReadWrite).unwrap();
    let c = p.counts().unwrap();
    let runs_after: i64 = p
        .conn()
        .query_row(
            "SELECT count(*) FROM runs WHERE stage = 'ocr' AND status = 'ok'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let redone = (runs_after - runs_before) as u32
        - (c.done - done_at_cancel).min((runs_after - runs_before) as u32);
    p.close().unwrap();
    Some(FullRun {
        source: "source-full.pdf".into(),
        source_blake3: blake3::hash(&bytes).to_hex().to_string(),
        pages,
        workers,
        model: model.subdir().into(),
        first_run_wall_s: w1,
        done_at_cancel,
        resume_wall_s: w2,
        done_after_resume: c.done,
        failed: f2,
        peak_rss_mb: peak1.max(peak2),
        redone_pages: redone,
    })
}

#[derive(Serialize)]
struct Report {
    generated: String,
    machine: Machine,
    models: Vec<(String, String)>,
    fixture: String,
    fixture_blake3: String,
    cache_state: &'static str,
    throughput: Vec<RunResult>,
    throughput_ratio_4_vs_1: f64,
    interaction: Option<Interaction>,
    export: Option<ExportBench>,
    full: Option<FullRun>,
    failures: Vec<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--worker") {
        std::process::exit(sbwb_worker::run_stdio_worker());
    }
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let pages: u32 = arg("--pages").and_then(|v| v.parse().ok()).unwrap_or(30);
    let out_dir = arg("--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("docs/benchmarks"));
    let full = args.iter().any(|a| a == "--full");
    let skip_throughput = args.iter().any(|a| a == "--no-throughput");
    std::fs::create_dir_all(&out_dir).unwrap();
    let fixture = root().join("fixtures/corpus/hough-1839-vol1/pages-001-110.pdf");
    let fixture_bytes = std::fs::read(&fixture).expect("fixture pdf");
    let models = vec![
        (
            "tessdata/best/eng.traineddata".to_string(),
            sha256_file(&root().join("models/best/eng.traineddata")),
        ),
        (
            "tessdata/fast/eng.traineddata".to_string(),
            sha256_file(&root().join("models/fast/eng.traineddata")),
        ),
        (
            "ocrs/text-detection.rten".to_string(),
            sha256_file(&root().join("models/ocrs/text-detection.rten")),
        ),
        (
            "ocrs/text-recognition.rten".to_string(),
            sha256_file(&root().join("models/ocrs/text-recognition.rten")),
        ),
        (
            "pdfium.dll".to_string(),
            sha256_file(&root().join("third_party/pdfium/bin/pdfium.dll")),
        ),
    ];
    let mut failures = Vec::new();
    let mut throughput = Vec::new();
    let tmp = tempfile::tempdir().unwrap();
    let mut interaction_result = None;
    if !skip_throughput {
        for workers in [1u32, 4] {
            let dir = tmp.path().join(format!("w{workers}"));
            std::fs::create_dir_all(&dir).unwrap();
            let (project, src) = make_project(
                &dir,
                &fixture,
                PageScope::from_ranges(vec![(48, 47 + pages)], 110),
                110,
            );
            eprintln!("throughput run: {pages} pages, {workers} worker(s)");
            let (wall, failed, done, peak) = run_pipeline(
                config(&project, &src, workers, sbwb_ocr::ModelPack::EngBest),
                None,
            );
            if failed > 0 {
                failures.push(format!("{failed} pages failed with {workers} workers"));
            }
            throughput.push(RunResult {
                label: format!("OCR+layout+text, {workers} worker(s)"),
                pages: done,
                workers,
                model: "eng best".into(),
                wall_s: wall,
                pages_per_min: done as f64 / (wall / 60.0),
                failed,
                peak_rss_mb: peak,
                ocr_secs_per_page: wall / done.max(1) as f64,
            });
            if workers == 4 {
                interaction_result = Some(interaction(&project, &src));
            }
        }
    }
    let ratio = match (throughput.first(), throughput.get(1)) {
        (Some(a), Some(b)) if a.pages_per_min > 0.0 => b.pages_per_min / a.pages_per_min,
        _ => 0.0,
    };
    let export = Some(export_bench(tmp.path()));
    let full = if full {
        full_run(tmp.path(), sbwb_pipeline::settings::effective_workers(6))
    } else {
        None
    };
    let report = Report {
        generated: jiff::Timestamp::now().to_string(),
        machine: machine(),
        models,
        fixture: "fixtures/corpus/hough-1839-vol1/pages-001-110.pdf".into(),
        fixture_blake3: blake3::hash(&fixture_bytes).to_hex().to_string(),
        cache_state: "cold (fresh project and render cache per run)",
        throughput,
        throughput_ratio_4_vs_1: ratio,
        interaction: interaction_result,
        export,
        full,
        failures,
    };
    let stamp = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    let stem = format!("{stamp}-{}", report.machine.host.to_lowercase());
    std::fs::write(
        out_dir.join(format!("{stem}.json")),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    std::fs::write(out_dir.join(format!("{stem}.md")), markdown(&report)).unwrap();
    println!("{}", markdown(&report));
}

fn markdown(r: &Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Benchmark {}\n\n", r.generated));
    s.push_str("Measured values on this machine; they are not advertised results (NFR-02).\n\n");
    s.push_str("| Item | Value |\n| --- | --- |\n");
    s.push_str(&format!("| Host | {} |\n| OS | {} |\n| CPU | {} ({} logical cores) |\n| Memory | {:.1} GB |\n| Build | {} · git {} · app {} |\n| Tesseract | {} |\n| Fixture | {} · blake3 {} |\n| Cache state | {} |\n",
        r.machine.host, r.machine.os, r.machine.cpu, r.machine.cores, r.machine.memory_gb, r.machine.build_profile, r.machine.git, r.machine.app_version, r.machine.tesseract, r.fixture, &r.fixture_blake3[..16], r.cache_state));
    for (name, sha) in &r.models {
        s.push_str(&format!(
            "| Model {name} | sha256 {}… |\n",
            &sha[..sha.len().min(16)]
        ));
    }
    if !r.throughput.is_empty() {
        s.push_str("\n## Throughput (NFR-07)\n\n| Run | Pages | Wall | Pages/min | s/page | Peak RSS | Failed |\n| --- | --- | --- | --- | --- | --- | --- |\n");
        for t in &r.throughput {
            s.push_str(&format!(
                "| {} | {} | {:.1} s | {:.1} | {:.2} | {:.0} MB | {} |\n",
                t.label,
                t.pages,
                t.wall_s,
                t.pages_per_min,
                t.ocr_secs_per_page,
                t.peak_rss_mb,
                t.failed
            ));
        }
        s.push_str(&format!(
            "\n4 workers vs 1: **{:.2}×** (target ≥ 2×).\n",
            r.throughput_ratio_4_vs_1
        ));
    }
    if let Some(i) = &r.interaction {
        s.push_str(&format!("\n## Interaction (NFR-03)\n\n| Measure | p95 | Target |\n| --- | --- | --- |\n| Project summary | {:.1} ms | ≤ 3000 ms |\n| Next issue ({} issues) | {:.1} ms | ≤ 300 ms |\n| Uncached preview render (1.5×) | {:.0} ms | ≤ 2000 ms |\n| Cached page read | {:.1} ms | ≤ 250 ms |\n", i.summary_ms_p95, i.issues, i.next_issue_ms_p95, i.uncached_preview_ms_p95, i.cached_page_ms_p95));
    }
    if let Some(e) = &r.export {
        s.push_str(&format!("\n## Export (NFR-05)\n\n{} words on {} pages with notes: **{:.1} s** including validation (target ≤ 60 s); validation {}.\n", e.words, e.pages, e.wall_s, if e.validation_ok { "passed" } else { "FAILED" }));
    }
    if let Some(f) = &r.full {
        s.push_str(&format!("\n## Large book (A-14, NFR-04)\n\n| Item | Value |\n| --- | --- |\n| Source | {} · blake3 {}… · {} pages |\n| Workers | {} · model {} |\n| First run | {:.0} s, interrupted after {} done |\n| Resume | {:.0} s, {} done at the end, {} failed |\n| Pages recognised twice | {} |\n| Peak aggregate RSS | {:.0} MB (target ≤ 6144 MB) |\n",
            f.source, &f.source_blake3[..16], f.pages, f.workers, f.model, f.first_run_wall_s, f.done_at_cancel, f.resume_wall_s, f.done_after_resume, f.failed, f.redone_pages, f.peak_rss_mb));
    }
    if !r.failures.is_empty() {
        s.push_str("\n## Failures\n\n");
        for f in &r.failures {
            s.push_str(&format!("- {f}\n"));
        }
    }
    s
}
