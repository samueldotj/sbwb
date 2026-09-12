//! M5.9 / NFR-09: held-out precision of automatic corrections (D-26).
//!
//! Runs OCR → layout → text pass on the ground-truth fixture pages and
//! writes every automatically applied change to a TSV for adjudication.
//! Verdicts already recorded in the fixture file are kept and merged, so
//! the file is safe to regenerate. Precision is reported separately for
//! user-checked and Claude-drafted verdicts; unadjudicated rows are
//! counted, never assumed correct.
//!
//! Output: `target/precision/auto-corrections.tsv` by default; with
//! `SBWB_UPDATE_GROUNDTRUTH=1` the fixture copy at
//! `fixtures/groundtruth/hough-1839-vol1/auto-corrections.tsv` is rewritten.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sbwb_core::{PageIndex, RunId, Transform};
use sbwb_text::{reconstruct, AutoApplyPolicy, Lexicon, PageTextInput};

const MIN_PRECISION: f64 = 0.99;

#[derive(Debug, Clone)]
struct Row {
    page: u32, // 1-based page number as printed in the fixture PDF
    kind: String,
    original: String,
    replacement: String,
    score: u8,
    reason: String,
    verdict: String, // ok | bad | ""
    by: String,      // user | claude | ""
    context: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_pages() -> Vec<u32> {
    let path = root().join("fixtures/groundtruth/hough-1839-vol1/layout.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["index"].as_u64().unwrap() as u32)
        .collect()
}

fn read_tsv(path: &std::path::Path) -> BTreeMap<(u32, String, String), Row> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        let row = Row {
            page: f[0].parse().unwrap_or(0),
            kind: f[1].into(),
            original: f[2].into(),
            replacement: f[3].into(),
            score: f[4].parse().unwrap_or(0),
            reason: f[5].into(),
            verdict: f[6].trim().into(),
            by: f[7].trim().into(),
            context: f.get(8).map(|c| c.to_string()).unwrap_or_default(),
        };
        out.insert(
            (row.page, row.original.clone(), row.replacement.clone()),
            row,
        );
    }
    out
}

fn write_tsv(path: &std::path::Path, rows: &[Row]) {
    let mut s =
        String::from("page\tkind\toriginal\treplacement\tscore\treason\tverdict\tby\tcontext\n");
    for r in rows {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.page,
            r.kind,
            r.original,
            r.replacement,
            r.score,
            r.reason.replace('\t', " "),
            r.verdict,
            r.by,
            r.context.replace('\t', " ")
        ));
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, s).unwrap();
}

#[test]
fn auto_correction_precision_on_groundtruth_pages() {
    let paths = sbwb_text::lexicon::default_paths();
    let pdfium = sbwb_pdf::default_pdfium_dir();
    let tessdata = sbwb_ocr::default_tessdata_root();
    let fixture = root().join("fixtures/corpus/hough-1839-vol1/pages-001-110.pdf");
    if !paths.hunspell_dic.exists()
        || !pdfium.join("pdfium.dll").exists()
        || !tessdata.join("fast/eng.traineddata").exists()
        || !fixture.exists()
    {
        eprintln!("skipping: deps missing");
        return;
    }
    let gt_path = root().join("fixtures/groundtruth/hough-1839-vol1/auto-corrections.tsv");
    let out_path = if std::env::var_os("SBWB_UPDATE_GROUNDTRUTH").is_some() {
        gt_path.clone()
    } else {
        root().join("target/precision/auto-corrections.tsv")
    };
    let previous = read_tsv(&gt_path);

    let lex = Lexicon::load(&paths).unwrap();
    let r = sbwb_pdf::Renderer::new(&pdfium).unwrap();
    let engine = sbwb_ocr::Engine::new(&tessdata);
    let policy = AutoApplyPolicy::default();
    let mut rows: Vec<Row> = Vec::new();
    let mut total_ms = 0u128;
    for page in fixture_pages() {
        let idx = PageIndex(page);
        let hi = r
            .render(
                &fixture,
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(idx, 300),
            )
            .unwrap();
        let ocr = engine
            .recognize(
                &hi.image,
                &hi.transform,
                &sbwb_ocr::OcrSettings {
                    model: sbwb_ocr::ModelPack::EngFast,
                    ..Default::default()
                },
            )
            .unwrap();
        let lo = r
            .render(
                &fixture,
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(idx, sbwb_layout::ANALYSIS_DPI),
            )
            .unwrap();
        let gray = image::imageops::grayscale(&lo.image);
        let t: Transform = lo.transform;
        let layout = sbwb_layout::analyze(
            &gray,
            &t,
            &sbwb_layout::LayoutInput {
                page_w: lo.page_size.0,
                page_h: lo.page_size.1,
                words: &ocr.words,
                lines: &ocr.lines,
                blocks: &ocr.blocks,
            },
            &sbwb_layout::AnalysisSettings::default(),
        );
        let input = PageTextInput {
            page: idx,
            run: RunId::new(),
            words: &ocr.words,
            regions: &layout.regions,
            page_w: lo.page_size.0,
            page_h: lo.page_size.1,
            previous_tail: None,
        };
        let t0 = std::time::Instant::now();
        let out = reconstruct(&input, &lex, &policy);
        let ms = t0.elapsed().as_millis();
        total_ms += ms;
        eprintln!(
            "page {} · {} words · {} applied · {} proposals · {ms} ms",
            page + 1,
            out.stats.words,
            out.stats.auto_applied,
            out.stats.proposals
        );
        for p in out.proposals.iter().filter(|p| p.auto_applied) {
            let key = (page + 1, p.original.clone(), p.replacement.clone());
            let (verdict, by) = previous
                .get(&key)
                .map(|r| (r.verdict.clone(), r.by.clone()))
                .unwrap_or_default();
            let context = out
                .spans
                .iter()
                .position(|s| s.id == p.span)
                .map(|i| {
                    let lo = i.saturating_sub(4);
                    let hi = (i + 5).min(out.spans.len());
                    out.spans[lo..hi]
                        .iter()
                        .map(|s| {
                            if s.id == p.span {
                                format!("[{}]", s.text)
                            } else {
                                s.text.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            rows.push(Row {
                page: page + 1,
                kind: serde_json::to_value(p.kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string(),
                original: p.original.clone(),
                replacement: p.replacement.clone(),
                score: p.score,
                reason: p.reason.clone(),
                verdict,
                by,
                context,
            });
        }
    }
    write_tsv(&out_path, &rows);

    let count = |by: &str, v: &str| rows.iter().filter(|r| r.by == by && r.verdict == v).count();
    let (user_ok, user_bad) = (count("user", "ok"), count("user", "bad"));
    let (claude_ok, claude_bad) = (count("claude", "ok"), count("claude", "bad"));
    let unadjudicated = rows.iter().filter(|r| r.verdict.is_empty()).count();
    let precision = |ok: usize, bad: usize| {
        if ok + bad == 0 {
            None
        } else {
            Some(ok as f64 / (ok + bad) as f64)
        }
    };
    eprintln!(
        "--- automatic corrections on {} ground-truth pages ({total_ms} ms in the text pass) ---",
        fixture_pages().len()
    );
    eprintln!("total {} · user-checked {} (precision {}) · claude-drafted {} (precision {}) · unadjudicated {}",
        rows.len(),
        user_ok + user_bad,
        precision(user_ok, user_bad).map(|p| format!("{:.1}%", p * 100.0)).unwrap_or("n/a".into()),
        claude_ok + claude_bad,
        precision(claude_ok, claude_bad).map(|p| format!("{:.1}%", p * 100.0)).unwrap_or("n/a".into()),
        unadjudicated);
    eprintln!("report written to {}", out_path.display());
    for r in rows.iter().filter(|r| r.verdict == "bad") {
        eprintln!(
            "BAD p.{} {} → {} [{}] {}",
            r.page, r.original, r.replacement, r.score, r.reason
        );
    }
    // Exit criterion (README M5): adjudicated precision at or above 99%,
    // otherwise auto-apply must ship as suggestion-only.
    if let Some(p) = precision(user_ok + claude_ok, user_bad + claude_bad) {
        assert!(
            p >= MIN_PRECISION,
            "auto-correction precision {:.1}% is below {:.0}%",
            p * 100.0,
            MIN_PRECISION * 100.0
        );
    }
}
