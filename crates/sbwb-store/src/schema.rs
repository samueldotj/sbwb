//! Versioned schema (PRJ-02). Every change is a new migration; the
//! `user_version` pragma records what a file has.

use rusqlite_migration::{Migrations, M};

/// Current project schema version. Bumped by every migration.
pub const SCHEMA_VERSION: u32 = 1;

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(V1)])
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
