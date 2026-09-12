//! Single-writer lock (PRJ-02). Stored inside the database so it travels
//! with the file and survives crashes: a stale heartbeat means the previous
//! writer died and the lock can be taken over.

use jiff::Timestamp;
use rusqlite::{params, Connection, OptionalExtension};
use sbwb_core::Result;
use serde::{Deserialize, Serialize};

const KEY: &str = "writer_lock";
/// A heartbeat older than this is considered abandoned.
pub const STALE_AFTER_SECS: i64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriterLock {
    pub pid: u32,
    pub host: String,
    pub started: Timestamp,
    pub heartbeat: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LockState {
    /// No lock or a stale one; a writer may take it.
    Free,
    /// Held by this process.
    Ours,
    /// Held by another live process.
    Other(WriterLock),
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "localhost".into())
}

pub fn read(conn: &Connection) -> Result<Option<WriterLock>> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM kv WHERE key = ?1", params![KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sbwb_core::SbwbError::other)?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn state(conn: &Connection, now: Timestamp) -> Result<LockState> {
    match read(conn)? {
        None => Ok(LockState::Free),
        Some(l) => {
            let age = now.as_second() - l.heartbeat.as_second();
            if l.pid == std::process::id() && l.host == hostname() {
                Ok(LockState::Ours)
            } else if age > STALE_AFTER_SECS {
                Ok(LockState::Free)
            } else {
                Ok(LockState::Other(l))
            }
        }
    }
}

/// Take the lock for this process. Fails only if another live writer holds it.
pub fn acquire(conn: &Connection, now: Timestamp) -> Result<LockState> {
    match state(conn, now)? {
        LockState::Other(l) => Ok(LockState::Other(l)),
        _ => {
            let lock = WriterLock {
                pid: std::process::id(),
                host: hostname(),
                started: now,
                heartbeat: now,
            };
            conn.execute(
                "INSERT INTO kv(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![KEY, serde_json::to_string(&lock)?],
            )
            .map_err(sbwb_core::SbwbError::other)?;
            Ok(LockState::Ours)
        }
    }
}

pub fn heartbeat(conn: &Connection, now: Timestamp) -> Result<()> {
    if let Some(mut l) = read(conn)? {
        if l.pid == std::process::id() && l.host == hostname() {
            l.heartbeat = now;
            conn.execute(
                "UPDATE kv SET value = ?2 WHERE key = ?1",
                params![KEY, serde_json::to_string(&l)?],
            )
            .map_err(sbwb_core::SbwbError::other)?;
        }
    }
    Ok(())
}

pub fn release(conn: &Connection) -> Result<()> {
    if let Some(l) = read(conn)? {
        if l.pid == std::process::id() && l.host == hostname() {
            conn.execute("DELETE FROM kv WHERE key = ?1", params![KEY])
                .map_err(sbwb_core::SbwbError::other)?;
        }
    }
    Ok(())
}

/// Write a foreign lock (tests and takeover simulations).
pub fn write_raw(conn: &Connection, lock: &WriterLock) -> Result<()> {
    conn.execute(
        "INSERT INTO kv(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, serde_json::to_string(lock)?],
    )
    .map_err(sbwb_core::SbwbError::other)?;
    Ok(())
}
