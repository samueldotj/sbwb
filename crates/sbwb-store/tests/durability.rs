//! M9.4 / NFR-10 durability matrix. Each test names the scenario it covers;
//! `docs/durability.md` maps the matrix to these tests and to the
//! scheduler and export tests that cover the rest.

use sbwb_core::{PageIndex, PageScope, PageStatus, SpanId, Stage};
use sbwb_store::{Decision, OpenMode, Project, SourceInfo};
use sbwb_text::{Span, SpanOrigin};
use std::path::Path;

fn create(dir: &Path) -> Project {
    let src = dir.join("book.pdf");
    let bytes = b"%PDF-1.4 durability".to_vec();
    std::fs::write(&src, &bytes).unwrap();
    let info = SourceInfo {
        name: "book.pdf".into(),
        size: bytes.len() as u64,
        blake3: blake3::hash(&bytes).to_hex().to_string(),
        page_count: 2,
        title: None,
        author: None,
    };
    let p = Project::create(
        &dir.join("book.sbwb"),
        &src,
        info,
        &[],
        PageScope::all(2),
        "test",
    )
    .unwrap();
    for i in 0..2 {
        p.set_page_status(PageIndex(i), PageStatus::Done, None)
            .unwrap();
        p.set_stage_done(PageIndex(i), Stage::TextPass, true)
            .unwrap();
    }
    p
}

fn span(page: u32, seq: u32, text: &str, conf: f32) -> Span {
    Span {
        id: SpanId::new(),
        page: PageIndex(page),
        seq,
        region: None,
        text: text.into(),
        anchors: vec![],
        origin: SpanOrigin::Ocr,
        confidence: Some(conf),
        trailing: " ".into(),
        revision: 1,
        protected: false,
        structure: Default::default(),
        paragraph_start: seq == 0,
    }
}

/// Crash: the process dies without `close()`. The acknowledged decision and
/// the unsaved draft both survive; the stale lock is taken over on reopen
/// and the draft is offered, not silently applied.
#[test]
fn crash_keeps_acknowledged_edits_and_recoverable_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.sbwb");
    let draft_span;
    {
        let mut p = create(dir.path());
        let spans = vec![span(0, 0, "carricd", 40.0), span(0, 1, "hom", 30.0)];
        p.put_page_text(PageIndex(0), None, &spans, &[]).unwrap();
        let issue = p.issues_for_page(PageIndex(0)).unwrap()[0].clone();
        p.decide_issue(
            issue.id,
            &Decision::Edit {
                text: "carried".into(),
            },
            1,
        )
        .unwrap();
        draft_span = spans[1].id;
        p.put_draft(draft_span, "home").unwrap();
        // simulate a crash: drop without close (the lock row stays behind)
        std::mem::drop(p);
    }
    // the same process id holds the stale lock: reopen succeeds read-write
    let p = Project::open(&path, OpenMode::ReadWrite).unwrap();
    assert!(!p.is_read_only());
    let s = p.page_spans(PageIndex(0)).unwrap();
    assert_eq!(s[0].text, "carried", "acknowledged edit survived");
    assert_eq!(s[1].text, "hom", "draft was not applied");
    let drafts = p.drafts(Some(PageIndex(0))).unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(
        (drafts[0].span, drafts[0].text.as_str()),
        (draft_span, "home")
    );
}

/// Read-only: a project opened read-only refuses every write with a clear
/// error, so a save failure is reported rather than pretended.
#[test]
fn read_only_open_refuses_writes_without_corrupting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.sbwb");
    {
        let mut p = create(dir.path());
        p.put_page_text(PageIndex(0), None, &[span(0, 0, "word", 50.0)], &[])
            .unwrap();
        p.close().unwrap();
    }
    let mut ro = Project::open(&path, OpenMode::ReadOnly).unwrap();
    assert!(ro.is_read_only());
    let issue = ro.issues_for_page(PageIndex(0)).unwrap()[0].clone();
    let err = ro.decide_issue(issue.id, &Decision::Skip, 1).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("read"), "{err}");
    assert!(ro.put_draft(issue.span, "x").is_err());
    let rw = Project::open(&path, OpenMode::ReadWrite).unwrap();
    assert_eq!(
        rw.issues_for_page(PageIndex(0)).unwrap()[0].status,
        sbwb_review::IssueStatus::Open
    );
}

/// Disk full / unwritable destination: `save_copy` to an impossible path
/// fails and leaves the live project intact and writable.
#[test]
fn save_copy_failure_leaves_the_project_intact() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = create(dir.path());
    p.put_page_text(PageIndex(0), None, &[span(0, 0, "word", 50.0)], &[])
        .unwrap();
    let bad = dir
        .path()
        .join("does-not-exist")
        .join("nope")
        .join("copy.sbwb");
    assert!(p.save_copy(&bad).is_err());
    assert!(!bad.exists());
    // still writable and consistent
    p.put_draft(p.page_spans(PageIndex(0)).unwrap()[0].id, "still fine")
        .unwrap();
    assert_eq!(p.drafts(None).unwrap().len(), 1);
}

/// Half-applied transactions: a grouped apply where one item is stale
/// changes nothing (covered in `review::tests`), and a cross-page join
/// rolls back entirely when the continuation span is missing.
#[test]
fn cross_page_join_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = create(dir.path());
    let a = span(0, 0, "communica-", 80.0);
    p.put_page_text(PageIndex(0), None, std::slice::from_ref(&a), &[])
        .unwrap();
    p.put_page_text(PageIndex(1), None, &[span(1, 0, "tion", 80.0)], &[])
        .unwrap();
    let ghost = sbwb_store::StoredProposal {
        id: sbwb_core::ProposalId::new(),
        page: PageIndex(0),
        span: a.id,
        span_revision: 1,
        kind: "hyphen_join".into(),
        original: "communica- / tion".into(),
        replacement: "communication".into(),
        score: 96,
        reason: "test".into(),
        source: "text_pass".into(),
        status: "open".into(),
        merged_span: Some(SpanId::new()), // continuation that does not exist
        cross_page: true,
    };
    assert!(
        !p.apply_cross_page_join(&ghost, PageIndex(1)).unwrap(),
        "nothing applied"
    );
    assert_eq!(p.page_spans(PageIndex(0)).unwrap()[0].text, "communica-");
    assert_eq!(p.page_spans(PageIndex(1)).unwrap()[0].text, "tion");
}
