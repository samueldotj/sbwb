//! A-12 (Word structure) under both furniture policies, the clean-copy
//! gate, and the M7.10 fixture timing.

use std::sync::atomic::AtomicBool;

use sbwb_export::test_support::sample_snapshot;
use sbwb_export::{run, CopyKind, FurniturePolicy, NoRenders, PageStructure};

fn export(
    snapshot: &sbwb_export::ExportSnapshot,
    name: &str,
) -> (sbwb_export::ExportReport, std::path::PathBuf) {
    let dir = std::env::temp_dir().join("sbwb-export-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let dest = dir.join(name);
    let _ = std::fs::remove_file(&dest);
    let cancel = AtomicBool::new(false);
    let mut phases = Vec::new();
    let report = run(snapshot, &dest, &NoRenders, &cancel, &mut |p, d, t| {
        phases.push((p, d, t))
    })
    .expect("export");
    assert!(phases
        .iter()
        .any(|(p, _, _)| *p == sbwb_export::Phase::Validate));
    (report, dest)
}

fn read_doc(path: &std::path::Path, entry: &str) -> String {
    let f = std::fs::File::open(path).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name(entry).unwrap(), &mut s).unwrap();
    s
}

#[test]
fn styled_policy_exports_furniture_styles_and_page_breaks() {
    let snap = sample_snapshot(3, false);
    let (report, dest) = export(&snap, "styled.docx");
    assert!(report.validation.ok, "{:?}", report.validation.checks);
    let doc = read_doc(&dest, "word/document.xml");
    assert!(doc.contains(r#"w:val="RunningHead""#));
    assert!(doc.contains(r#"w:val="PrintedPageNumber""#));
    assert!(doc.contains(r#"w:val="SideNote""#));
    assert!(doc.contains(r#"w:val="FootnoteText""#));
    assert_eq!(
        doc.matches(r#"w:type="page""#).count(),
        2,
        "two page breaks for three pages"
    );
    assert!(!doc.contains("[Source page"));
    assert!(doc.contains("communication"));
    assert_eq!(
        doc.matches("communication").count(),
        1,
        "joined word not duplicated"
    );
    // working copy: flags → comments and highlights
    assert!(report.comments > 0 && report.comments == report.highlights);
    let comments = read_doc(&dest, "word/comments.xml");
    assert!(comments.contains("low OCR confidence"));
    let core = read_doc(&dest, "docProps/core.xml");
    assert!(core.contains("<dc:title>The History of Christianity in India</dc:title>"));
    assert!(core.contains("<dc:creator>James Hough</dc:creator>"));
    assert!(report.exclusions.iter().any(|e| e.kind == "catchword"));
    assert!(std::fs::metadata(dest.with_extension("export-report.json")).is_ok());
    assert_eq!(report.checksum_blake3.len(), 64);
}

#[test]
fn native_policy_exports_sections_with_headers_and_footers() {
    let mut snap = sample_snapshot(3, false);
    snap.settings.furniture = FurniturePolicy::NativeHeadersFooters;
    snap.settings.copy = CopyKind::Clean;
    snap.settings.archive = true;
    let (report, dest) = export(&snap, "native.docx");
    assert!(report.validation.ok, "{:?}", report.validation.checks);
    let doc = read_doc(&dest, "word/document.xml");
    assert_eq!(doc.matches("<w:sectPr").count(), 3, "one section per page");
    assert_eq!(doc.matches(r#"w:val="nextPage""#).count(), 3);
    assert_eq!(doc.matches("<w:headerReference").count(), 3);
    assert_eq!(doc.matches("<w:footerReference").count(), 3);
    assert!(
        !doc.contains(r#"w:val="RunningHead""#),
        "no running head paragraphs in the body"
    );
    let h1 = read_doc(&dest, "word/header1.xml");
    let h2 = read_doc(&dest, "word/header2.xml");
    assert!(
        h1.contains("CHRISTIANITY") && h2.contains("INDIA") && !h2.contains("CHRISTIANITY"),
        "page-specific headers do not inherit"
    );
    let f1 = read_doc(&dest, "word/footerS1.xml");
    let f_last = read_doc(&dest, "word/footer1.xml");
    assert!(
        f1.contains("A.D. 1500") && f_last.contains("1502"),
        "side notes in page-specific footers"
    );
    // clean copy: nothing review-related
    assert_eq!(report.comments, 0);
    assert!(!doc.contains("<w:highlight"));
    assert!(report
        .archive_path
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false));
    let arch = std::fs::File::open(report.archive_path.unwrap()).unwrap();
    let z = zip::ZipArchive::new(arch).unwrap();
    let names: Vec<&str> = z.file_names().collect();
    assert!(
        names.contains(&"transcript.json")
            && names.contains(&"layout/page-0001.xml")
            && names.contains(&"manifest.json")
            && names.contains(&"validation.json")
    );
}

#[test]
fn continuous_text_states_its_furniture_fallback() {
    let mut snap = sample_snapshot(2, false);
    snap.settings.structure = PageStructure::Continuous;
    snap.settings.furniture = FurniturePolicy::NativeHeadersFooters;
    let (report, dest) = export(&snap, "continuous.docx");
    assert!(report.validation.ok, "{:?}", report.validation.checks);
    let doc = read_doc(&dest, "word/document.xml");
    assert_eq!(doc.matches(r#"w:type="page""#).count(), 0);
    assert_eq!(doc.matches("<w:sectPr").count(), 1);
    assert!(report.exclusions.iter().any(|e| e.kind == "header"));
    assert!(report.warnings.iter().any(|w| w.contains("fell back")));
}

#[test]
fn clean_copy_is_gated_on_approval() {
    let mut snap = sample_snapshot(2, false);
    snap.settings.copy = CopyKind::Clean;
    snap.pages[1].approval = "outdated".into();
    let dest = std::env::temp_dir().join("sbwb-export-tests/gated.docx");
    let _ = std::fs::remove_file(&dest);
    let err = run(
        &snap,
        &dest,
        &NoRenders,
        &AtomicBool::new(false),
        &mut |_, _, _| {},
    )
    .unwrap_err();
    assert!(err.to_string().contains("1 page left"), "{err}");
    assert!(!dest.exists(), "nothing published");
}

#[test]
fn drop_cap_and_letter_preset_keep_parity() {
    let mut snap = sample_snapshot(2, false);
    snap.settings.preset.drop_cap.enabled = true;
    snap.settings.preset.drop_cap.lines = 3;
    snap.settings.preset.paper = sbwb_export::settings::PaperSize::Letter;
    snap.settings.metadata.title = Some("Custom Title".into());
    let (report, dest) = export(&snap, "dropcap.docx");
    assert!(report.validation.ok, "{:?}", report.validation.checks);
    assert_eq!(report.drop_caps, 2);
    let doc = read_doc(&dest, "word/document.xml");
    assert!(doc.contains("<w:framePr"));
    assert!(
        doc.contains(r#"w:w="12240" w:h="15840""#),
        "letter size on the section"
    );
    assert!(read_doc(&dest, "docProps/core.xml").contains("<dc:title>Custom Title</dc:title>"));
}

/// M7.10 / NFR-05: 500 pages × ~500 words with notes; target 60 s for
/// creation and validation. Debug builds are slower; the assertion is on
/// release-like budgets only when `SBWB_PERF=1`.
#[test]
fn large_fixture_exports_within_budget() {
    let snap = sample_snapshot(500, true);
    assert!(snap.word_count() >= 250_000, "{} words", snap.word_count());
    let t = std::time::Instant::now();
    let (report, _) = export(&snap, "large.docx");
    let secs = t.elapsed().as_secs_f64();
    eprintln!(
        "250k-word export: {secs:.1} s · {} paragraphs · {} comments",
        report.stats.paragraphs, report.comments
    );
    assert!(report.validation.ok);
    if std::env::var_os("SBWB_PERF").is_some() {
        assert!(secs <= 60.0, "{secs:.1} s");
    }
}
