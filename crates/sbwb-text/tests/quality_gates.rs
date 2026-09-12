//! M9.3 / NFR-08: frozen recognition-quality gates on the annotated page.
//! CER and WER of the effective text (OCR → layout → text pass, no human
//! decisions) against the transcription in `fixtures/groundtruth/.../text`,
//! by region class. The numbers were measured on 2026-09-12 with the
//! `eng_fast` model at 300 dpi and frozen with headroom; a regression fails
//! the test, an improvement is reported.

use sbwb_core::{PageIndex, RunId, Transform};
use sbwb_layout::RegionKind;
use sbwb_text::{reconstruct, AutoApplyPolicy, Lexicon, PageTextInput};

/// Frozen gates (fraction). Body text is the release-relevant class.
const BODY_CER_MAX: f64 = 0.010; // measured 0.31%
const BODY_WER_MAX: f64 = 0.025; // measured 0.80%
const NOTES_WER_MAX: f64 = 0.25; // measured 15.4% (13 words)
const MARGINALIA_WER_MAX: f64 = 0.25; // measured 13.3% (15 words)

fn norm_chars(s: &str) -> Vec<char> {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '’' => '\'',
            '“' | '”' => '"',
            c => c,
        })
        .collect()
}

fn norm_words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'')
                .replace('’', "'")
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn levenshtein<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn rates(hyp: &str, truth: &str) -> (f64, f64, usize) {
    let (hc, tc) = (norm_chars(hyp), norm_chars(truth));
    let (hw, tw) = (norm_words(hyp), norm_words(truth));
    let cer = levenshtein(&hc, &tc) as f64 / tc.len().max(1) as f64;
    let wer = levenshtein(&hw, &tw) as f64 / tw.len().max(1) as f64;
    (cer, wer, tw.len())
}

#[test]
fn effective_text_meets_frozen_gates_on_page_48() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = sbwb_text::lexicon::default_paths();
    let pdfium = sbwb_pdf::default_pdfium_dir();
    let tessdata = sbwb_ocr::default_tessdata_root();
    let fixture = root.join("fixtures/corpus/hough-1839-vol1/pages-001-110.pdf");
    let gt_path = root.join("fixtures/groundtruth/hough-1839-vol1/text/page-048.json");
    if !paths.hunspell_dic.exists()
        || !pdfium.join("pdfium.dll").exists()
        || !tessdata.join("fast/eng.traineddata").exists()
        || !fixture.exists()
    {
        eprintln!("skipping: deps missing");
        return;
    }
    let gt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&gt_path).unwrap()).unwrap();
    let page = PageIndex(gt["index"].as_u64().unwrap() as u32);
    let lex = Lexicon::load(&paths).unwrap();
    let r = sbwb_pdf::Renderer::new(&pdfium).unwrap();
    let hi = r
        .render(
            &fixture,
            None,
            &sbwb_pdf::RenderRequest::page_at_dpi(page, 300),
        )
        .unwrap();
    let ocr = sbwb_ocr::Engine::new(&tessdata)
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
            &sbwb_pdf::RenderRequest::page_at_dpi(page, sbwb_layout::ANALYSIS_DPI),
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
        page,
        run: RunId::new(),
        words: &ocr.words,
        regions: &layout.regions,
        page_w: lo.page_size.0,
        page_h: lo.page_size.1,
        previous_tail: None,
    };
    let out = reconstruct(&input, &lex, &AutoApplyPolicy::default());

    // effective text per region class
    let text_of = |kinds: &[RegionKind]| -> String {
        let ids: Vec<_> = layout
            .regions
            .iter()
            .filter(|r| kinds.contains(&r.kind))
            .map(|r| r.id)
            .collect();
        out.spans
            .iter()
            .filter(|s| s.region.map(|r| ids.contains(&r)).unwrap_or(false))
            .map(|s| format!("{} ", s.text))
            .collect::<String>()
    };
    let strip_markers = |s: &str| -> String {
        // OCR renders superscript footnote markers as stray quotes/digits after a full stop
        s.replace(".'", ".")
            .replace(".’", ".")
            .replace(",²", ",")
            .replace(",2", ",")
            .replace(".¹", ".")
            .replace(".1 ", ". ")
    };
    let body_truth = gt["regions"]["body"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let notes_truth = gt["regions"]["footnotes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let marg_truth = gt["regions"]["marginalia"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    let body = strip_markers(&text_of(&[RegionKind::Body, RegionKind::Heading]));
    let notes = strip_markers(&text_of(&[RegionKind::Footnote]));
    let marg = text_of(&[RegionKind::Marginalia]);
    let (bcer, bwer, bn) = rates(&body, &body_truth);
    let (ncer, nwer, nn) = rates(&notes, &notes_truth);
    let (mcer, mwer, mn) = rates(&marg, &marg_truth);
    eprintln!(
        "page 48 body: CER {:.2}% WER {:.2}% over {bn} words",
        bcer * 100.0,
        bwer * 100.0
    );
    eprintln!(
        "page 48 footnotes: CER {:.2}% WER {:.2}% over {nn} words",
        ncer * 100.0,
        nwer * 100.0
    );
    eprintln!(
        "page 48 marginalia: CER {:.2}% WER {:.2}% over {mn} words",
        mcer * 100.0,
        mwer * 100.0
    );
    assert!(bn > 250, "body ground truth loaded");
    assert!(
        bcer <= BODY_CER_MAX,
        "body CER {:.2}% above the frozen gate {:.1}%",
        bcer * 100.0,
        BODY_CER_MAX * 100.0
    );
    assert!(
        bwer <= BODY_WER_MAX,
        "body WER {:.2}% above the frozen gate {:.1}%",
        bwer * 100.0,
        BODY_WER_MAX * 100.0
    );
    assert!(
        nwer <= NOTES_WER_MAX,
        "footnote WER {:.2}% above the gate",
        nwer * 100.0
    );
    assert!(
        mwer <= MARGINALIA_WER_MAX,
        "marginalia WER {:.2}% above the gate",
        mwer * 100.0
    );
    // coverage: every ground-truth region class received text
    assert!(
        !marg.trim().is_empty() && !notes.trim().is_empty(),
        "marginalia and footnotes covered"
    );
}
