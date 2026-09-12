//! Layout ground truth (M4.8, NFR-08, D-26).
//!
//! `fixtures/groundtruth/hough-1839-vol1/layout.json` holds region
//! annotations for a sample of fixture pages. The first draft was produced
//! by the analyser and visually checked on page 48 only; the file header
//! records which pages a person has adjudicated. This test reports region
//! precision/recall per class (matching by IoU >= 0.5) against that file and
//! fails when recall of adjudicated pages drops.
//!
//! Regenerate the draft with `SBWB_WRITE_GT=1 cargo test -p sbwb-layout --test groundtruth`.

use std::path::PathBuf;

use sbwb_core::{PageIndex, Rect, Transform};
use sbwb_layout::{analyze, AnalysisSettings, LayoutInput, Region, RegionKind};
use serde::{Deserialize, Serialize};

const PAGES: [u32; 10] = [9, 19, 29, 39, 47, 54, 60, 69, 84, 99];

#[derive(Debug, Serialize, Deserialize)]
struct GtFile {
    note: String,
    adjudicated_pages: Vec<u32>,
    pages: Vec<GtPage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GtPage {
    index: u32,
    page_w: f64,
    page_h: f64,
    regions: Vec<GtRegion>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GtRegion {
    kind: RegionKind,
    order: u32,
    bbox: Rect,
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
}

fn gt_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/groundtruth/hough-1839-vol1/layout.json")
}

fn analyze_page(
    r: &sbwb_pdf::Renderer,
    engine: &sbwb_ocr::Engine,
    idx: u32,
) -> (Vec<Region>, f64, f64) {
    let hi = r
        .render(
            &fixture(),
            None,
            &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(idx), 300),
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
            &fixture(),
            None,
            &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(idx), sbwb_layout::ANALYSIS_DPI),
        )
        .unwrap();
    let gray = image::imageops::grayscale(&lo.image);
    let input = LayoutInput {
        page_w: lo.page_size.0,
        page_h: lo.page_size.1,
        words: &ocr.words,
        lines: &ocr.lines,
        blocks: &ocr.blocks,
    };
    let t: Transform = lo.transform;
    let out = analyze(&gray, &t, &input, &AnalysisSettings::default());
    (out.regions, lo.page_size.0, lo.page_size.1)
}

fn iou(a: &Rect, b: &Rect) -> f64 {
    let inter = a.intersection(b).map(|r| r.area()).unwrap_or(0.0);
    let union = a.area() + b.area() - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

#[test]
fn layout_matches_ground_truth_sample() {
    let pdfium = sbwb_pdf::default_pdfium_dir();
    let tessdata = sbwb_ocr::default_tessdata_root();
    if !pdfium.join("pdfium.dll").exists() || !tessdata.join("fast/eng.traineddata").exists() {
        eprintln!("skipping: deps missing");
        return;
    }
    let r = sbwb_pdf::Renderer::new(&pdfium).unwrap();
    let engine = sbwb_ocr::Engine::new(&tessdata);

    if std::env::var_os("SBWB_WRITE_GT").is_some() {
        let mut pages = Vec::new();
        for idx in PAGES {
            let (regions, w, h) = analyze_page(&r, &engine, idx);
            pages.push(GtPage {
                index: idx,
                page_w: w,
                page_h: h,
                regions: regions
                    .iter()
                    .map(|g| GtRegion {
                        kind: g.kind,
                        order: g.order,
                        bbox: g.bbox,
                    })
                    .collect(),
            });
        }
        let file = GtFile {
            note: "Draft produced by sbwb-layout cc-smear/1 on 2026-09-12. Pages listed in adjudicated_pages were checked by a person; the rest are unadjudicated (D-26).".into(),
            adjudicated_pages: vec![47],
            pages,
        };
        std::fs::create_dir_all(gt_path().parent().unwrap()).unwrap();
        std::fs::write(gt_path(), serde_json::to_string_pretty(&file).unwrap()).unwrap();
        eprintln!("wrote {}", gt_path().display());
        return;
    }

    let Ok(raw) = std::fs::read_to_string(gt_path()) else {
        eprintln!("skipping: no ground truth file at {}", gt_path().display());
        return;
    };
    let gt: GtFile = serde_json::from_str(&raw).unwrap();
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fnr = 0usize;
    let mut order_errors = 0usize;
    let mut adjudicated_missed = 0usize;
    for page in &gt.pages {
        let (regions, _, _) = analyze_page(&r, &engine, page.index);
        let mut matched = vec![false; regions.len()];
        for g in &page.regions {
            let best = regions
                .iter()
                .enumerate()
                .filter(|(i, _)| !matched[*i])
                .map(|(i, r)| (i, iou(&g.bbox, &r.bbox), r))
                .filter(|(_, v, r)| *v >= 0.5 && r.kind == g.kind)
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            match best {
                Some((i, _, r)) => {
                    matched[i] = true;
                    tp += 1;
                    if r.order != g.order {
                        order_errors += 1;
                    }
                }
                None => {
                    fnr += 1;
                    if gt.adjudicated_pages.contains(&page.index) {
                        adjudicated_missed += 1;
                        eprintln!("p{}: missed {:?} at {:?}", page.index + 1, g.kind, g.bbox);
                    }
                }
            }
        }
        fp += matched.iter().filter(|m| !**m).count();
    }
    let precision = tp as f64 / (tp + fp).max(1) as f64;
    let recall = tp as f64 / (tp + fnr).max(1) as f64;
    eprintln!(
        "layout vs ground truth: {} pages · precision {:.2} · recall {:.2} · order errors {} · adjudicated pages {:?}",
        gt.pages.len(),
        precision,
        recall,
        order_errors,
        gt.adjudicated_pages
    );
    assert_eq!(adjudicated_missed, 0, "regions missed on adjudicated pages");
    assert!(recall >= 0.9, "recall {recall:.2} against the frozen draft");
}
