//! Project file API.

use std::io::Read;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use rusqlite::{params, Connection, OptionalExtension};
use sbwb_core::{PageIndex, PageScope, PageStatus, ProjectId, Result, RunId, SbwbError, Stage};
use serde::{Deserialize, Serialize};

use crate::lock::{self, LockState};
use crate::schema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Take the writer lock; fails if another live process holds it.
    ReadWrite,
    /// Never write. Used when another instance holds the lock (PRJ-02).
    ReadOnly,
}

/// Facts about the source PDF recorded at import (PRJ-01).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceInfo {
    pub name: String,
    pub size: u64,
    pub blake3: String,
    pub page_count: u32,
    pub title: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectMeta {
    pub id: ProjectId,
    pub created_at: Timestamp,
    pub app_version: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub source: SourceInfo,
    pub scope: PageScope,
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageRow {
    pub index: PageIndex,
    pub width_pt: Option<f64>,
    pub height_pt: Option<f64>,
    pub status: PageStatus,
    pub printed_label: Option<String>,
    pub error: Option<String>,
    pub approved_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRecord {
    pub id: RunId,
    pub stage: Stage,
    pub page: Option<PageIndex>,
    pub engine: Option<String>,
    pub model: Option<String>,
    pub settings: serde_json::Value,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub status: String,
    pub error: Option<String>,
    pub elapsed_ms: Option<u64>,
}

/// What the Welcome page and title bar need (PRJ-03, design 4.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSummary {
    pub path: PathBuf,
    pub meta: ProjectMeta,
    pub read_only: bool,
    pub pages: Vec<PageRow>,
    pub counts: PageCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PageCounts {
    pub source: u32,
    pub in_scope: u32,
    pub unprocessed: u32,
    pub excluded: u32,
    pub queued: u32,
    pub running: u32,
    pub failed: u32,
    pub done: u32,
    pub approved: u32,
}

/// Current OCR evidence for a page: run, words, lines, mean confidence.
pub type PageOcr = (RunId, Vec<sbwb_ocr::OcrWord>, Vec<sbwb_ocr::OcrLine>, f32);

pub struct Project {
    conn: Connection,
    path: PathBuf,
    mode: OpenMode,
}

fn status_str(s: PageStatus) -> &'static str {
    match s {
        PageStatus::Unprocessed => "unprocessed",
        PageStatus::Excluded => "excluded",
        PageStatus::Queued => "queued",
        PageStatus::Running => "running",
        PageStatus::Failed => "failed",
        PageStatus::Done => "done",
    }
}

fn parse_status(s: &str) -> PageStatus {
    match s {
        "excluded" => PageStatus::Excluded,
        "queued" => PageStatus::Queued,
        "running" => PageStatus::Running,
        "failed" => PageStatus::Failed,
        "done" => PageStatus::Done,
        _ => PageStatus::Unprocessed,
    }
}

fn stage_str(s: Stage) -> &'static str {
    match s {
        Stage::Import => "import",
        Stage::Ocr => "ocr",
        Stage::Layout => "layout",
        Stage::TextPass => "text_pass",
        Stage::AiProofread => "ai_proofread",
        Stage::Export => "export",
    }
}

fn parse_stage(s: &str) -> Stage {
    match s {
        "ocr" => Stage::Ocr,
        "layout" => Stage::Layout,
        "text_pass" => Stage::TextPass,
        "ai_proofread" => Stage::AiProofread,
        "export" => Stage::Export,
        _ => Stage::Import,
    }
}

fn db(e: rusqlite::Error) -> SbwbError {
    SbwbError::other(format!("database: {e}"))
}

impl Project {
    /// Create a new project file at `path` from a source PDF, storing a
    /// verified copy of the PDF inside (PRJ-01). `scope` is the initial
    /// processing scope (D-21: first 50 pages by default).
    pub fn create(
        path: &Path,
        source_pdf: &Path,
        source: SourceInfo,
        page_sizes: &[(f64, f64)],
        scope: PageScope,
        app_version: &str,
    ) -> Result<Self> {
        if path.exists() {
            return Err(SbwbError::Conflict(format!(
                "{} already exists",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Read and re-hash the source so the stored blob is verified.
        let mut bytes = Vec::with_capacity(source.size as usize);
        std::fs::File::open(source_pdf)?.read_to_end(&mut bytes)?;
        let actual = blake3::hash(&bytes).to_hex().to_string();
        if actual != source.blake3 {
            return Err(SbwbError::Integrity(format!(
                "source changed while importing (expected {}, got {})",
                &source.blake3[..12],
                &actual[..12]
            )));
        }

        let mut conn = Connection::open(path).map_err(db)?;
        Self::configure(&conn)?;
        schema::migrations()
            .to_latest(&mut conn)
            .map_err(|e| SbwbError::other(format!("schema: {e}")))?;

        let now = Timestamp::now();
        let id = ProjectId::new();
        let tx = conn.transaction().map_err(db)?;
        tx.execute(
            "INSERT INTO project(id, created_at, app_version, title, author, source_name, source_size, source_blake3, source_page_count, page_scope, settings)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id.to_string(),
                now.to_string(),
                app_version,
                source.title,
                source.author,
                source.name,
                source.size as i64,
                source.blake3,
                source.page_count as i64,
                serde_json::to_string(&scope)?,
                "{}",
            ],
        )
        .map_err(db)?;
        tx.execute(
            "INSERT INTO source_blob(id, data) VALUES (1, ?1)",
            params![bytes],
        )
        .map_err(db)?;
        {
            let mut stmt = tx
                .prepare("INSERT INTO pages(page_index, width_pt, height_pt, status, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)")
                .map_err(db)?;
            for i in 0..source.page_count {
                let p = PageIndex(i);
                let status = if scope.contains(p) {
                    PageStatus::Queued
                } else {
                    PageStatus::Unprocessed
                };
                let (w, h) = page_sizes
                    .get(i as usize)
                    .copied()
                    .map(|(w, h)| (Some(w), Some(h)))
                    .unwrap_or((None, None));
                stmt.execute(params![i as i64, w, h, status_str(status), now.to_string()])
                    .map_err(db)?;
            }
        }
        tx.commit().map_err(db)?;
        lock::acquire(&conn, now)?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
            mode: OpenMode::ReadWrite,
        })
    }

    /// Open an existing project. Read-only opens never write, not even the
    /// lock. A newer schema than this build understands is refused without
    /// modification (PRJ-02).
    pub fn open(path: &Path, mode: OpenMode) -> Result<Self> {
        if !path.is_file() {
            return Err(SbwbError::NotFound(path.display().to_string()));
        }
        let flags = match mode {
            OpenMode::ReadWrite => {
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            }
            OpenMode::ReadOnly => {
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            }
        };
        let mut conn = Connection::open_with_flags(path, flags)
            .map_err(|e| SbwbError::invalid(format!("cannot open project: {e}")))?;
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db)?;
        if version > schema::SCHEMA_VERSION {
            return Err(SbwbError::invalid(format!(
                "project schema {version} is newer than this version of SBWB (supports {})",
                schema::SCHEMA_VERSION
            )));
        }
        if version == 0 {
            // Not a project (or an empty database).
            let has_project: bool = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='project'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .map(|n| n > 0)
                .unwrap_or(false);
            if !has_project {
                return Err(SbwbError::invalid("not an SBWB project file"));
            }
        }
        match mode {
            OpenMode::ReadWrite => {
                Self::configure(&conn)?;
                if version < schema::SCHEMA_VERSION {
                    Self::backup_before_migration(path)?;
                    schema::migrations()
                        .to_latest(&mut conn)
                        .map_err(|e| SbwbError::other(format!("schema upgrade: {e}")))?;
                }
                if let LockState::Other(l) = lock::acquire(&conn, Timestamp::now())? {
                    return Err(SbwbError::Conflict(format!(
                        "project is open for writing by process {} on {}",
                        l.pid, l.host
                    )));
                }
            }
            OpenMode::ReadOnly => {
                if version < schema::SCHEMA_VERSION {
                    return Err(SbwbError::invalid(
                        "project needs a schema upgrade; open it for writing first",
                    ));
                }
            }
        }
        Ok(Self {
            conn,
            path: path.to_path_buf(),
            mode,
        })
    }

    /// Lock state as another process would see it (for read-only fallback).
    pub fn peek_lock(path: &Path) -> Result<LockState> {
        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| SbwbError::invalid(format!("cannot open project: {e}")))?;
        lock::state(&conn, Timestamp::now())
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
            .map_err(db)
    }

    fn backup_before_migration(path: &Path) -> Result<()> {
        let mut backup = path.as_os_str().to_os_string();
        backup.push(format!(".pre-v{}.bak", schema::SCHEMA_VERSION));
        std::fs::copy(path, PathBuf::from(backup))?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn is_read_only(&self) -> bool {
        self.mode == OpenMode::ReadOnly
    }
    pub fn cache_dir(&self) -> PathBuf {
        crate::cache_dir_for(&self.path)
    }

    fn require_write(&self) -> Result<()> {
        if self.is_read_only() {
            Err(SbwbError::Conflict("project is open read-only".into()))
        } else {
            Ok(())
        }
    }

    pub fn meta(&self) -> Result<ProjectMeta> {
        self.conn
            .query_row(
                "SELECT id, created_at, app_version, title, author, source_name, source_size, source_blake3, source_page_count, page_scope, settings FROM project",
                [],
                |r| {
                    let id: String = r.get(0)?;
                    let created: String = r.get(1)?;
                    let scope: String = r.get(9)?;
                    let settings: String = r.get(10)?;
                    Ok(ProjectMeta {
                        id: ProjectId::parse(&id).unwrap_or_default(),
                        created_at: created.parse().unwrap_or_else(|_| Timestamp::now()),
                        app_version: r.get(2)?,
                        title: r.get(3)?,
                        author: r.get(4)?,
                        source: SourceInfo {
                            name: r.get(5)?,
                            size: r.get::<_, i64>(6)? as u64,
                            blake3: r.get(7)?,
                            page_count: r.get::<_, i64>(8)? as u32,
                            title: r.get(3)?,
                            author: r.get(4)?,
                        },
                        scope: serde_json::from_str(&scope).unwrap_or_default(),
                        settings: serde_json::from_str(&settings).unwrap_or(serde_json::Value::Null),
                    })
                },
            )
            .map_err(db)
    }

    pub fn set_title(&self, title: Option<&str>, author: Option<&str>) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "UPDATE project SET title = ?1, author = ?2",
                params![title, author],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn settings(&self) -> Result<serde_json::Value> {
        Ok(self.meta()?.settings)
    }

    pub fn set_settings(&self, settings: &serde_json::Value) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "UPDATE project SET settings = ?1",
                params![serde_json::to_string(settings)?],
            )
            .map_err(db)?;
        Ok(())
    }

    /// Replace the processing scope. Pages entering the scope become Queued;
    /// pages leaving it become Unprocessed unless they already have results
    /// (which are kept, PRJ-03).
    pub fn set_scope(&mut self, scope: &PageScope) -> Result<()> {
        self.require_write()?;
        let now = Timestamp::now().to_string();
        let total = self.meta()?.source.page_count;
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "UPDATE project SET page_scope = ?1",
            params![serde_json::to_string(scope)?],
        )
        .map_err(db)?;
        {
            let mut stmt = tx.prepare("UPDATE pages SET status = ?2, updated_at = ?3 WHERE page_index = ?1 AND status = ?4").map_err(db)?;
            for i in 0..total {
                let p = PageIndex(i);
                if scope.contains(p) {
                    stmt.execute(params![i as i64, "queued", now, "unprocessed"])
                        .map_err(db)?;
                } else {
                    stmt.execute(params![i as i64, "unprocessed", now, "queued"])
                        .map_err(db)?;
                }
            }
        }
        tx.commit().map_err(db)
    }

    pub fn pages(&self) -> Result<Vec<PageRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT page_index, width_pt, height_pt, status, printed_label, error, approved_revision FROM pages ORDER BY page_index")
            .map_err(db)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PageRow {
                    index: PageIndex(r.get::<_, i64>(0)? as u32),
                    width_pt: r.get(1)?,
                    height_pt: r.get(2)?,
                    status: parse_status(&r.get::<_, String>(3)?),
                    printed_label: r.get(4)?,
                    error: r.get(5)?,
                    approved_revision: r.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    pub fn set_page_status(
        &self,
        page: PageIndex,
        status: PageStatus,
        error: Option<&str>,
    ) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "UPDATE pages SET status = ?2, error = ?3, updated_at = ?4 WHERE page_index = ?1",
                params![
                    page.0 as i64,
                    status_str(status),
                    error,
                    Timestamp::now().to_string()
                ],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn set_page_size(&self, page: PageIndex, width_pt: f64, height_pt: f64) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "UPDATE pages SET width_pt = ?2, height_pt = ?3 WHERE page_index = ?1",
                params![page.0 as i64, width_pt, height_pt],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn counts(&self) -> Result<PageCounts> {
        let meta = self.meta()?;
        let mut c = PageCounts {
            source: meta.source.page_count,
            in_scope: meta.scope.len(),
            ..Default::default()
        };
        for p in self.pages()? {
            match p.status {
                PageStatus::Unprocessed => c.unprocessed += 1,
                PageStatus::Excluded => c.excluded += 1,
                PageStatus::Queued => c.queued += 1,
                PageStatus::Running => c.running += 1,
                PageStatus::Failed => c.failed += 1,
                PageStatus::Done => c.done += 1,
            }
            if p.approved_revision.is_some() {
                c.approved += 1;
            }
        }
        Ok(c)
    }

    pub fn summary(&self) -> Result<ProjectSummary> {
        Ok(ProjectSummary {
            path: self.path.clone(),
            meta: self.meta()?,
            read_only: self.is_read_only(),
            pages: self.pages()?,
            counts: self.counts()?,
        })
    }

    // ----- source PDF -----

    /// Verify the stored source blob against its recorded checksum (PRJ-01).
    pub fn verify_source(&self) -> Result<()> {
        let expected = self.meta()?.source.blake3;
        let actual = self.hash_blob()?;
        if actual != expected {
            return Err(SbwbError::Integrity(format!(
                "stored source hash {} differs from recorded {}",
                &actual[..12],
                &expected[..12]
            )));
        }
        Ok(())
    }

    fn hash_blob(&self) -> Result<String> {
        let blob = self
            .conn
            .blob_open(rusqlite::MAIN_DB, "source_blob", "data", 1, true)
            .map_err(db)?;
        let mut hasher = blake3::Hasher::new();
        let mut reader = std::io::BufReader::with_capacity(1 << 20, blob);
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Path of the extracted source PDF in the cache directory, extracting
    /// (or re-extracting after a mismatch) as needed. Renderers work from
    /// this file; the blob stays the authority.
    pub fn source_path(&self) -> Result<PathBuf> {
        let meta = self.meta()?;
        let dir = self.cache_dir();
        std::fs::create_dir_all(&dir)?;
        let target = dir.join("source.pdf");
        if target.is_file() {
            if let Ok(h) = sbwb_file_hash(&target) {
                if h == meta.source.blake3 {
                    return Ok(target);
                }
            }
        }
        self.verify_source()?;
        let blob = self
            .conn
            .blob_open(rusqlite::MAIN_DB, "source_blob", "data", 1, true)
            .map_err(db)?;
        let tmp = dir.join("source.pdf.part");
        {
            let mut out = std::fs::File::create(&tmp)?;
            let mut reader = std::io::BufReader::with_capacity(1 << 20, blob);
            std::io::copy(&mut reader, &mut out)?;
        }
        std::fs::rename(&tmp, &target)?;
        Ok(target)
    }

    // ----- runs and OCR evidence -----

    pub fn start_run(
        &self,
        stage: Stage,
        page: Option<PageIndex>,
        engine: Option<&str>,
        model: Option<&str>,
        settings: &serde_json::Value,
    ) -> Result<RunId> {
        self.require_write()?;
        let id = RunId::new();
        self.conn
            .execute(
                "INSERT INTO runs(id, stage, page_index, engine, model, settings, started_at, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running')",
                params![id.to_string(), stage_str(stage), page.map(|p| p.0 as i64), engine, model, serde_json::to_string(settings)?, Timestamp::now().to_string()],
            )
            .map_err(db)?;
        Ok(id)
    }

    pub fn finish_run(
        &self,
        id: RunId,
        status: &str,
        error: Option<&str>,
        elapsed_ms: Option<u64>,
    ) -> Result<()> {
        self.require_write()?;
        self.conn
            .execute(
                "UPDATE runs SET finished_at = ?2, status = ?3, error = ?4, elapsed_ms = ?5 WHERE id = ?1",
                params![id.to_string(), Timestamp::now().to_string(), status, error, elapsed_ms.map(|v| v as i64)],
            )
            .map_err(db)?;
        Ok(())
    }

    pub fn runs_for_page(&self, page: PageIndex) -> Result<Vec<RunRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, stage, page_index, engine, model, settings, started_at, finished_at, status, error, elapsed_ms FROM runs WHERE page_index = ?1 ORDER BY started_at")
            .map_err(db)?;
        let rows = stmt
            .query_map(params![page.0 as i64], |r| {
                let id: String = r.get(0)?;
                let settings: String = r.get(5)?;
                let started: String = r.get(6)?;
                let finished: Option<String> = r.get(7)?;
                Ok(RunRecord {
                    id: RunId::parse(&id).unwrap_or_default(),
                    stage: parse_stage(&r.get::<_, String>(1)?),
                    page: r.get::<_, Option<i64>>(2)?.map(|v| PageIndex(v as u32)),
                    engine: r.get(3)?,
                    model: r.get(4)?,
                    settings: serde_json::from_str(&settings).unwrap_or(serde_json::Value::Null),
                    started_at: started.parse().unwrap_or_else(|_| Timestamp::now()),
                    finished_at: finished.and_then(|f| f.parse().ok()),
                    status: r.get(8)?,
                    error: r.get(9)?,
                    elapsed_ms: r.get::<_, Option<i64>>(10)?.map(|v| v as u64),
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    /// Store OCR output for a page as the current evidence; earlier runs are
    /// kept (PIPE-01) but marked not current.
    pub fn put_page_ocr(
        &mut self,
        page: PageIndex,
        run: RunId,
        output: &sbwb_ocr::OcrOutput,
    ) -> Result<()> {
        self.require_write()?;
        let tx = self.conn.transaction().map_err(db)?;
        tx.execute(
            "UPDATE page_ocr SET current = 0 WHERE page_index = ?1",
            params![page.0 as i64],
        )
        .map_err(db)?;
        tx.execute(
            "INSERT INTO page_ocr(page_index, run_id, words, lines, blocks, tsv, mean_confidence, current) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                page.0 as i64,
                run.to_string(),
                serde_json::to_string(&output.words)?,
                serde_json::to_string(&output.lines)?,
                serde_json::to_string(&output.blocks)?,
                output.tsv,
                output.mean_confidence as f64,
            ],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }

    pub fn current_page_ocr(&self, page: PageIndex) -> Result<Option<PageOcr>> {
        let row = self
            .conn
            .query_row(
                "SELECT run_id, words, lines, mean_confidence FROM page_ocr WHERE page_index = ?1 AND current = 1",
                params![page.0 as i64],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<f64>>(3)?)),
            )
            .optional()
            .map_err(db)?;
        match row {
            None => Ok(None),
            Some((run, words, lines, conf)) => Ok(Some((
                RunId::parse(&run).unwrap_or_default(),
                serde_json::from_str(&words)?,
                serde_json::from_str(&lines)?,
                conf.unwrap_or(0.0) as f32,
            ))),
        }
    }

    // ----- history -----

    pub fn add_history(
        &self,
        kind: &str,
        label: &str,
        payload: &serde_json::Value,
    ) -> Result<sbwb_core::HistoryId> {
        self.require_write()?;
        let id = sbwb_core::HistoryId::new();
        self.conn
            .execute(
                "INSERT INTO history(id, ts, kind, label, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id.to_string(),
                    Timestamp::now().to_string(),
                    kind,
                    label,
                    serde_json::to_string(payload)?
                ],
            )
            .map_err(db)?;
        Ok(id)
    }

    // ----- lifecycle -----

    pub fn heartbeat(&self) -> Result<()> {
        if self.is_read_only() {
            return Ok(());
        }
        lock::heartbeat(&self.conn, Timestamp::now())
    }

    /// Write a consistent portable copy (PRJ-02). Uses `VACUUM INTO`, which
    /// snapshots the database without an exclusive lock.
    pub fn save_copy(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            return Err(SbwbError::Conflict(format!(
                "{} already exists",
                dest.display()
            )));
        }
        let tmp = dest.with_extension("sbwb.part");
        let _ = std::fs::remove_file(&tmp);
        self.conn
            .execute("VACUUM INTO ?1", params![tmp.to_string_lossy().to_string()])
            .map_err(db)?;
        // The copy must not carry our writer lock.
        {
            let c = Connection::open(&tmp).map_err(db)?;
            c.execute("DELETE FROM kv WHERE key = 'writer_lock'", [])
                .map_err(db)?;
        }
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }

    /// Release the writer lock and close.
    pub fn close(self) -> Result<()> {
        if !self.is_read_only() {
            lock::release(&self.conn)?;
            let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        Ok(())
    }

    /// The underlying connection, for repositories in other crates.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

fn sbwb_file_hash(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::WriterLock;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
    }

    fn source_info() -> SourceInfo {
        let bytes = std::fs::read(fixture()).unwrap();
        SourceInfo {
            name: "pages-001-110.pdf".into(),
            size: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            page_count: 110,
            title: Some("The History of Christianity in India".into()),
            author: Some("James Hough".into()),
        }
    }

    fn create_in(dir: &Path) -> Project {
        Project::create(
            &dir.join("book.sbwb"),
            &fixture(),
            source_info(),
            &[(355.0, 606.0)],
            PageScope::first_n(50, 110),
            "test",
        )
        .unwrap()
    }

    #[test]
    fn create_reopen_and_summary() {
        let dir = tempfile::tempdir().unwrap();
        let p = create_in(dir.path());
        let s = p.summary().unwrap();
        assert_eq!(s.meta.source.page_count, 110);
        assert_eq!(s.counts.queued, 50);
        assert_eq!(s.counts.unprocessed, 60);
        assert_eq!(s.pages[0].width_pt, Some(355.0));
        p.close().unwrap();

        let p = Project::open(&dir.path().join("book.sbwb"), OpenMode::ReadWrite).unwrap();
        p.verify_source().unwrap();
        let src = p.source_path().unwrap();
        assert!(src.ends_with("source.pdf"));
        assert_eq!(std::fs::metadata(&src).unwrap().len(), source_info().size);
        assert_eq!(p.meta().unwrap().author.as_deref(), Some("James Hough"));
    }

    #[test]
    fn scope_extension_keeps_results() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = create_in(dir.path());
        p.set_page_status(PageIndex(0), PageStatus::Done, None)
            .unwrap();
        p.set_scope(&PageScope::first_n(110, 110)).unwrap();
        let c = p.counts().unwrap();
        assert_eq!(c.done, 1);
        assert_eq!(c.queued, 109);
        assert_eq!(c.unprocessed, 0);
        // shrinking back never touches finished pages
        p.set_scope(&PageScope::first_n(1, 110)).unwrap();
        let c = p.counts().unwrap();
        assert_eq!(c.done, 1);
        assert_eq!(c.unprocessed, 109);
    }

    #[test]
    fn integrity_mismatch_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let p = create_in(dir.path());
        p.conn()
            .execute(
                "UPDATE project SET source_blake3 = 'deadbeef' || substr(source_blake3, 9)",
                [],
            )
            .unwrap();
        assert!(matches!(p.verify_source(), Err(SbwbError::Integrity(_))));
        assert!(matches!(p.source_path(), Err(SbwbError::Integrity(_))));
    }

    #[test]
    fn foreign_live_lock_forces_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.sbwb");
        let p = create_in(dir.path());
        p.close().unwrap();
        {
            let c = Connection::open(&path).unwrap();
            let now = Timestamp::now();
            lock::write_raw(
                &c,
                &WriterLock {
                    pid: 999_999,
                    host: "elsewhere".into(),
                    started: now,
                    heartbeat: now,
                },
            )
            .unwrap();
        }
        assert!(matches!(
            Project::peek_lock(&path).unwrap(),
            LockState::Other(_)
        ));
        assert!(matches!(
            Project::open(&path, OpenMode::ReadWrite),
            Err(SbwbError::Conflict(_))
        ));
        let ro = Project::open(&path, OpenMode::ReadOnly).unwrap();
        assert!(ro.is_read_only());
        assert!(ro
            .set_page_status(PageIndex(0), PageStatus::Done, None)
            .is_err());
        // a stale lock is free to take
        {
            let c = Connection::open(&path).unwrap();
            let old = Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_secs(600))
                .unwrap();
            lock::write_raw(
                &c,
                &WriterLock {
                    pid: 999_999,
                    host: "elsewhere".into(),
                    started: old,
                    heartbeat: old,
                },
            )
            .unwrap();
        }
        assert!(matches!(
            Project::peek_lock(&path).unwrap(),
            LockState::Free
        ));
        assert!(Project::open(&path, OpenMode::ReadWrite).is_ok());
    }

    #[test]
    fn save_copy_is_consistent_and_unlocked() {
        let dir = tempfile::tempdir().unwrap();
        let p = create_in(dir.path());
        let copy = dir.path().join("copy.sbwb");
        p.save_copy(&copy).unwrap();
        assert!(matches!(
            Project::peek_lock(&copy).unwrap(),
            LockState::Free
        ));
        let c = Project::open(&copy, OpenMode::ReadOnly).unwrap();
        assert_eq!(c.counts().unwrap().source, 110);
        c.verify_source().unwrap();
    }

    #[test]
    fn rejects_non_project_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.sbwb");
        std::fs::write(&p, b"hello").unwrap();
        assert!(Project::open(&p, OpenMode::ReadOnly).is_err());
        let db = Connection::open(dir.path().join("empty.sbwb")).unwrap();
        drop(db);
        assert!(matches!(
            Project::open(&dir.path().join("empty.sbwb"), OpenMode::ReadWrite),
            Err(SbwbError::InvalidInput(_))
        ));
    }

    #[test]
    fn ocr_evidence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = create_in(dir.path());
        let run = p
            .start_run(
                Stage::Ocr,
                Some(PageIndex(3)),
                Some("tesseract"),
                Some("eng_best"),
                &serde_json::json!({"dpi": 300}),
            )
            .unwrap();
        let t = sbwb_core::Transform::for_dpi(300);
        let page = sbwb_ocr::parse_tsv("level\tp\tb\tpar\tl\tw\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t30\t40\t88\tIndia\n", &t);
        let out = sbwb_ocr::OcrOutput {
            words: page.words,
            lines: page.lines,
            blocks: page.blocks,
            tsv: "raw".into(),
            engine: "tesseract".into(),
            model: sbwb_ocr::ModelPack::EngBest,
            settings: sbwb_ocr::OcrSettings::default(),
            mean_confidence: 88.0,
        };
        p.put_page_ocr(PageIndex(3), run, &out).unwrap();
        p.finish_run(run, "ok", None, Some(1200)).unwrap();
        let (r, words, _, conf) = p.current_page_ocr(PageIndex(3)).unwrap().unwrap();
        assert_eq!(r, run);
        assert_eq!(words[0].text, "India");
        assert!((conf - 88.0).abs() < 1e-3);
        let runs = p.runs_for_page(PageIndex(3)).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].elapsed_ms, Some(1200));
    }
}
