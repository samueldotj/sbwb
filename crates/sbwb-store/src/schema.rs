//! Versioned schema (PRJ-02). Every change is a new migration; the
//! `user_version` pragma records what a file has.

use rusqlite_migration::{Migrations, M};

/// Current project schema version. Bumped by every migration.
pub const SCHEMA_VERSION: u32 = 5;

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(V1), M::up(V2), M::up(V3), M::up(V4), M::up(V5)])
}

const V1: &str = r#"
CREATE TABLE project (
  id TEXT PRIMARY KEY,
  created_at TEXT NOT NULL,
  app_version TEXT NOT NULL,
  title TEXT,
  author TEXT,
  source_name TEXT NOT NULL,
  source_size INTEGER NOT NULL,
  source_blake3 TEXT NOT NULL,
  source_page_count INTEGER NOT NULL,
  page_scope TEXT NOT NULL,
  settings TEXT NOT NULL
);

CREATE TABLE source_blob (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  data BLOB NOT NULL
);

CREATE TABLE pages (
  page_index INTEGER PRIMARY KEY,
  width_pt REAL,
  height_pt REAL,
  status TEXT NOT NULL,
  printed_label TEXT,
  error TEXT,
  approved_revision INTEGER,
  updated_at TEXT NOT NULL
);

CREATE TABLE runs (
  id TEXT PRIMARY KEY,
  stage TEXT NOT NULL,
  page_index INTEGER,
  engine TEXT,
  model TEXT,
  settings TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  status TEXT NOT NULL,
  error TEXT,
  elapsed_ms INTEGER
);
CREATE INDEX runs_page ON runs(page_index, stage);

CREATE TABLE page_ocr (
  page_index INTEGER NOT NULL,
  run_id TEXT NOT NULL,
  words TEXT NOT NULL,
  lines TEXT NOT NULL,
  blocks TEXT NOT NULL,
  tsv TEXT NOT NULL,
  mean_confidence REAL,
  current INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (page_index, run_id)
);
CREATE INDEX page_ocr_current ON page_ocr(page_index, current);

CREATE TABLE history (
  id TEXT PRIMARY KEY,
  ts TEXT NOT NULL,
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  payload TEXT NOT NULL,
  undone INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE exports (
  id TEXT PRIMARY KEY,
  ts TEXT NOT NULL,
  kind TEXT NOT NULL,
  path TEXT NOT NULL,
  checksum TEXT,
  settings TEXT NOT NULL,
  report TEXT
);

CREATE TABLE kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

/// v2: per-stage completion flags, layout results (M4).
const V2: &str = r#"
ALTER TABLE pages ADD COLUMN ocr_done INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pages ADD COLUMN layout_done INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pages ADD COLUMN text_done INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pages ADD COLUMN layout_revision INTEGER NOT NULL DEFAULT 0;
UPDATE pages SET ocr_done = 1 WHERE status = 'done';

CREATE TABLE page_layout (
  page_index INTEGER PRIMARY KEY,
  run_id TEXT,
  regions TEXT NOT NULL,
  report TEXT NOT NULL,
  algorithm TEXT,
  manual INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);
"#;

/// v3: effective text spans, proposals, project vocabulary (M5).
const V3: &str = r#"
CREATE TABLE spans (
  id TEXT PRIMARY KEY,
  page_index INTEGER NOT NULL,
  seq INTEGER NOT NULL,
  region_id TEXT,
  text TEXT NOT NULL,
  trailing TEXT NOT NULL,
  origin TEXT NOT NULL,
  confidence REAL,
  anchors TEXT NOT NULL,
  revision INTEGER NOT NULL,
  protected INTEGER NOT NULL DEFAULT 0,
  structure TEXT NOT NULL,
  paragraph_start INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX spans_page ON spans(page_index, seq);

CREATE TABLE proposals (
  id TEXT PRIMARY KEY,
  page_index INTEGER NOT NULL,
  span_id TEXT NOT NULL,
  span_revision INTEGER NOT NULL,
  kind TEXT NOT NULL,
  original TEXT NOT NULL,
  replacement TEXT NOT NULL,
  score INTEGER NOT NULL,
  reason TEXT NOT NULL,
  source TEXT NOT NULL,
  status TEXT NOT NULL,
  merged_span TEXT,
  cross_page INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  decided_at TEXT,
  run_id TEXT
);
CREATE INDEX proposals_page ON proposals(page_index, status);

CREATE TABLE vocab (
  word TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  added_at TEXT NOT NULL
);

ALTER TABLE pages ADD COLUMN text_revision INTEGER NOT NULL DEFAULT 0;
"#;

/// v4: the review issue index, approvals with acknowledgements, drafts (M6).
const V4: &str = r#"
CREATE TABLE issues (
  id TEXT PRIMARY KEY,
  page_index INTEGER NOT NULL,
  seq INTEGER NOT NULL,
  span_id TEXT NOT NULL,
  span_revision INTEGER NOT NULL,
  kind TEXT NOT NULL,
  score INTEGER,
  score_source TEXT NOT NULL,
  priority INTEGER NOT NULL,
  proposal_id TEXT,
  original TEXT NOT NULL,
  replacement TEXT,
  reason TEXT NOT NULL,
  status TEXT NOT NULL,
  decision TEXT,
  note TEXT,
  candidates TEXT NOT NULL,
  created_at TEXT NOT NULL,
  decided_at TEXT,
  history_id TEXT
);
CREATE INDEX issues_nav ON issues(status, page_index, seq);
CREATE INDEX issues_prio ON issues(status, priority DESC, page_index, seq);
CREATE INDEX issues_span ON issues(span_id);
CREATE INDEX issues_page ON issues(page_index);

CREATE TABLE drafts (
  span_id TEXT PRIMARY KEY,
  page_index INTEGER NOT NULL,
  text TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

ALTER TABLE pages ADD COLUMN approved_outstanding INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pages ADD COLUMN approved_layout_revision INTEGER;
ALTER TABLE pages ADD COLUMN approved_at TEXT;
"#;

/// v5: structural issues carry a box and a region (M8.2).
const V5: &str = r#"
ALTER TABLE issues ADD COLUMN bbox TEXT;
ALTER TABLE issues ADD COLUMN region_id TEXT;
"#;
