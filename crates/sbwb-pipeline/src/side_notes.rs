//! Side notes the page OCR missed (OCR-02).
//!
//! Tesseract's page segmentation (PSM 3) drops small heavy blocks such as
//! marginal notes: it returns no box for them at all, so a marginal region
//! can end up with no word inside. Each such region is read again on its
//! own as one block of text (PSM 6) and the words join the page's OCR
//! evidence, where the text pass picks them up like any other word.

use std::path::Path;
use std::time::Instant;

use sbwb_core::{PageIndex, Result, SbwbError, Stage};
use sbwb_layout::{Region, RegionKind};
use sbwb_ocr::{ModelPack, OcrSettings, OcrWord, Segmentation};
use sbwb_store::Project;
use sbwb_worker::{RequestKind, Response, Worker};

/// Crops are rendered at 300 DPI and enlarged 2× (Lanczos3): small side-note
/// type reads best there; 3× starts to break letters apart.
pub const DPI: u32 = 300;
pub const ENLARGE: u32 = 2;

/// Words from one side-note pass, ready to store.
#[derive(Debug, Clone, Default)]
pub struct SideNoteReading {
    pub words: Vec<OcrWord>,
    /// Per region: id, crop and the recorded inputs.
    pub profiles: Vec<serde_json::Value>,
    pub elapsed_ms: u64,
}

/// Marginal regions with no recognised word inside.
pub fn empty_side_notes(regions: &[Region], words: &[OcrWord]) -> Vec<Region> {
    regions
        .iter()
        .filter(|r| r.kind == RegionKind::Marginalia && r.bbox.w > 2.0 && r.bbox.h > 2.0)
        .filter(|r| {
            !words
                .iter()
                .any(|w| !w.text.trim().is_empty() && sbwb_text::reconstruct::inside(w, r))
        })
        .cloned()
        .collect()
}

/// Read each region on the worker as a single block. Words outside their
/// region (from the crop padding) are left out.
pub fn read_side_notes(
    worker: &mut Worker,
    source: &Path,
    page: PageIndex,
    notes: &[Region],
    model: ModelPack,
) -> Result<SideNoteReading> {
    let started = Instant::now();
    let mut out = SideNoteReading::default();
    for (k, r) in notes.iter().enumerate() {
        let resp = worker.call(
            RequestKind::OcrRegion {
                path: source.to_path_buf(),
                password: None,
                page,
                crop: r.bbox,
                dpi: DPI,
                enlarge: ENLARGE,
                settings: OcrSettings {
                    model,
                    segmentation: Segmentation::SingleBlock,
                    ..Default::default()
                },
                second_engine: false,
                save_render: None,
            },
            |_| {},
        )?;
        let (output, profile) = match resp {
            Response::RegionOcr {
                output, profile, ..
            } => (output, profile),
            other => return Err(SbwbError::other(format!("unexpected reply {other:?}"))),
        };
        let words: Vec<OcrWord> = output
            .words
            .into_iter()
            .filter(|w| !w.text.trim().is_empty() && sbwb_text::reconstruct::inside(w, r))
            .map(|mut w| {
                // Keep each note's lines apart from the page's own blocks.
                w.block += 1000 * (k as u32 + 1);
                w
            })
            .collect();
        out.profiles.push(serde_json::json!({
            "region": r.id,
            "words": words.len(),
            "profile": profile,
        }));
        out.words.extend(words);
    }
    out.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(out)
}

/// Record the pass as an OCR run, add its words to the page's OCR evidence
/// and invalidate the page's text. Returns the number of words added.
pub fn store_side_notes(
    project: &mut Project,
    page: PageIndex,
    model: ModelPack,
    reading: &SideNoteReading,
) -> Result<u32> {
    let settings = serde_json::json!({
        "kind": "side_notes",
        "segmentation": "single_block",
        "dpi": DPI,
        "enlarge": ENLARGE,
        "regions": reading.profiles,
        "words": reading.words,
    });
    let run = project.start_run(
        Stage::Ocr,
        Some(page),
        Some("tesseract"),
        Some(model.subdir()),
        &settings,
    )?;
    let n = reading.words.len() as u32;
    if n > 0 {
        project.append_page_ocr_words(page, &reading.words)?;
        project.set_stage_done(page, Stage::TextPass, false)?;
        project.add_history(
            "side_notes",
            &format!("Side notes read · page {} · {n} words", page.number()),
            &serde_json::json!({ "page": page.0, "run": run, "words": n }),
        )?;
    }
    project.finish_run(run, "ok", None, Some(reading.elapsed_ms))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    use sbwb_core::{PageScope, Rect};
    use sbwb_store::SourceInfo;
    use sbwb_worker::WorkerConfig;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
    }

    fn region(kind: RegionKind, bbox: Rect, order: u32) -> Region {
        let mut r = Region::new(kind, bbox, order);
        r.manual = true;
        r
    }

    /// Page 45 of the Hough corpus: "The Saviour's command to His apostles."
    /// stands in the right margin in small bold type. Page OCR returns no
    /// box for it; reading the drawn region on its own recovers it.
    #[test]
    fn page_45_side_note_missed_by_page_ocr_is_read() {
        let Some(exe) = std::env::var_os("SBWB_TEST_WORKER_EXE").map(PathBuf::from) else {
            eprintln!("skipping: SBWB_TEST_WORKER_EXE not set");
            return;
        };
        let mut worker = Worker::spawn(WorkerConfig {
            exe: Some(exe),
            timeout: Duration::from_secs(120),
            grace: Duration::from_millis(300),
            pdfium_dir: Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/bin"),
            ),
            tessdata_dir: Some(sbwb_ocr::default_tessdata_root()),
        })
        .unwrap();
        let page = PageIndex(44);
        let model = ModelPack::EngBest;
        let output = match worker
            .call(
                RequestKind::OcrPage {
                    path: fixture(),
                    password: None,
                    page,
                    dpi: DPI,
                    settings: OcrSettings {
                        model,
                        ..Default::default()
                    },
                    prep: Default::default(),
                    save_render: None,
                },
                |_| {},
            )
            .unwrap()
        {
            Response::Ocr { output, .. } => output,
            other => panic!("unexpected reply {other:?}"),
        };
        let note = region(
            RegionKind::Marginalia,
            Rect::new(270.4, 207.6, 31.0, 47.3),
            1,
        );
        let regions = vec![
            region(RegionKind::Body, Rect::new(5.6, 64.6, 256.1, 447.4), 0),
            note.clone(),
        ];
        // The root cause: page segmentation (PSM 3) leaves the note empty.
        let empty = empty_side_notes(&regions, &output.words);
        assert_eq!(empty.len(), 1, "page OCR read the side note after all");

        let dir = tempfile::tempdir().unwrap();
        let bytes = std::fs::read(fixture()).unwrap();
        let info = SourceInfo {
            name: "fixture.pdf".into(),
            size: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            page_count: 110,
            title: None,
            author: None,
        };
        let mut project = Project::create(
            &dir.path().join("book.sbwb"),
            &fixture(),
            info,
            &[],
            PageScope::from_ranges(vec![(45, 45)], 110),
            "test",
        )
        .unwrap();
        let source = project.source_path().unwrap();
        let run = project
            .start_run(
                Stage::Ocr,
                Some(page),
                Some("tesseract"),
                Some("best"),
                &serde_json::json!({}),
            )
            .unwrap();
        project.put_page_ocr(page, run, &output).unwrap();
        project
            .put_page_layout(
                page,
                None,
                &serde_json::to_value(&regions).unwrap(),
                &serde_json::json!({}),
                Some("manual"),
                true,
                true,
            )
            .unwrap();

        let reading = read_side_notes(&mut worker, &source, page, &empty, model).unwrap();
        let n = store_side_notes(&mut project, page, model, &reading).unwrap();
        worker.shutdown();

        let (_, words, _, _) = project.current_page_ocr(page).unwrap().unwrap();
        assert_eq!(words.len(), output.words.len() + n as usize);
        assert!(empty_side_notes(&regions, &words).is_empty());
        let text: Vec<&str> = words
            .iter()
            .filter(|w| sbwb_text::reconstruct::inside(w, &note))
            .map(|w| w.text.as_str())
            .collect();
        let text = text.join(" ");
        for want in ["The", "command", "to", "apostles."] {
            assert!(
                text.split(' ').any(|t| t == want),
                "{want} missing from {text:?}"
            );
        }
    }
}
