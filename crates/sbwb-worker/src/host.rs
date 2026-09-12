//! Host side: spawn a worker, send requests, enforce timeouts (NFR-06).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};
use sbwb_core::{Result, SbwbError};

use crate::protocol::{Message, Request, RequestKind, Response};

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Executable to launch; defaults to the current executable.
    pub exe: Option<PathBuf>,
    /// Per-request timeout (NFR-06: 120 s initially).
    pub timeout: Duration,
    /// Grace period after a cancel/timeout before the process is killed.
    pub grace: Duration,
    pub pdfium_dir: Option<PathBuf>,
    pub tessdata_dir: Option<PathBuf>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            exe: None,
            timeout: Duration::from_secs(120),
            grace: Duration::from_secs(10),
            pdfium_dir: None,
            tessdata_dir: None,
        }
    }
}

pub struct Worker {
    child: Child,
    stdin: std::process::ChildStdin,
    rx: Receiver<Message>,
    next_id: AtomicU64,
    config: WorkerConfig,
    pub pid: u32,
}

impl Worker {
    pub fn spawn(config: WorkerConfig) -> Result<Self> {
        let exe = match &config.exe {
            Some(e) => e.clone(),
            None => std::env::current_exe()?,
        };
        let mut child = Command::new(&exe)
            .arg("--worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| SbwbError::other(format!("spawn worker {}: {e}", exe.display())))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("worker-reader".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Message>(&line) {
                        Ok(m) => {
                            if tx.send(m).is_err() {
                                break;
                            }
                        }
                        Err(e) => tracing::warn!("worker emitted unparsable line: {e}: {line}"),
                    }
                }
            })
            .map_err(SbwbError::other)?;
        let pid = child.id();
        let mut w = Self {
            child,
            stdin,
            rx,
            next_id: AtomicU64::new(1),
            config: config.clone(),
            pid,
        };
        if config.pdfium_dir.is_some() || config.tessdata_dir.is_some() {
            w.call(
                RequestKind::Configure {
                    pdfium_dir: config
                        .pdfium_dir
                        .clone()
                        .unwrap_or_else(sbwb_pdf::default_pdfium_dir),
                    tessdata_dir: config
                        .tessdata_dir
                        .clone()
                        .unwrap_or_else(sbwb_ocr::default_tessdata_root),
                },
                |_| {},
            )?;
        }
        Ok(w)
    }

    /// Send one request and wait for its result, forwarding progress to
    /// `on_progress`. Times out per [`WorkerConfig::timeout`]; on timeout the
    /// worker is killed after the grace period and the error is returned.
    pub fn call(
        &mut self,
        kind: RequestKind,
        mut on_progress: impl FnMut(&str),
    ) -> Result<Response> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = Request { id, kind };
        let line = serde_json::to_string(&req)?;
        writeln!(self.stdin, "{line}")
            .map_err(|e| SbwbError::other(format!("worker write: {e}")))?;
        self.stdin
            .flush()
            .map_err(|e| SbwbError::other(format!("worker flush: {e}")))?;
        let deadline = Instant::now() + self.config.timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                self.escalate_kill();
                return Err(SbwbError::Timeout(self.config.timeout));
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(Message::Result { id: rid, result }) if rid == id => return Ok(result),
                Ok(Message::Error { id: rid, error }) if rid == id => {
                    return Err(SbwbError::Other(error))
                }
                Ok(Message::Progress { id: rid, activity }) if rid == id => on_progress(&activity),
                Ok(Message::Heartbeat { .. }) => {}
                Ok(other) => tracing::debug!("stale worker message {:?}", other.id()),
                Err(RecvTimeoutError::Timeout) => {
                    self.escalate_kill();
                    return Err(SbwbError::Timeout(self.config.timeout));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(SbwbError::other("worker process exited unexpectedly"));
                }
            }
        }
    }

    pub fn ping(&mut self) -> Result<u32> {
        match self.call(RequestKind::Ping, |_| {})? {
            Response::Pong { pid } => Ok(pid),
            other => Err(SbwbError::other(format!("unexpected reply {other:?}"))),
        }
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Ask the worker to stop; kill it if it does not exit within the grace
    /// period (NFR-06 escalation).
    pub fn shutdown(mut self) {
        let _ = writeln!(
            self.stdin,
            "{}",
            serde_json::to_string(&Request {
                id: 0,
                kind: RequestKind::Shutdown
            })
            .unwrap()
        );
        let _ = self.stdin.flush();
        self.escalate_kill();
    }

    fn escalate_kill(&mut self) {
        let deadline = Instant::now() + self.config.grace;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.is_alive() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test binary itself acts as the worker when run with --worker; the
    /// test harness forwards through `worker_main` in this crate's tests.
    fn test_exe() -> PathBuf {
        std::env::current_exe().unwrap()
    }

    #[test]
    fn ping_roundtrip_and_timeout() {
        // When the test binary is started with --worker, `run_stdio_worker` must
        // run instead of the test harness; see tests/worker_bin.rs for the
        // executable used here.
        let exe = std::env::var_os("SBWB_TEST_WORKER_EXE").map(PathBuf::from);
        let Some(exe) = exe else {
            eprintln!("skipping: SBWB_TEST_WORKER_EXE not set");
            return;
        };
        let _ = test_exe();
        let cfg = WorkerConfig {
            exe: Some(exe),
            timeout: Duration::from_millis(600),
            grace: Duration::from_millis(200),
            ..Default::default()
        };
        let mut w = Worker::spawn(cfg).unwrap();
        let pid = w.ping().unwrap();
        assert_eq!(pid, w.pid);
        let err = w
            .call(RequestKind::Sleep { ms: 5_000 }, |_| {})
            .unwrap_err();
        assert!(matches!(err, SbwbError::Timeout(_)));
        assert!(
            !w.is_alive(),
            "worker should have been killed after the grace period"
        );
    }
}
