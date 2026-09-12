//! Worker process boundary (M0.9; NFR-06, NFR-11).
//!
//! The application launches copies of its own executable with `--worker`.
//! Each worker speaks newline-delimited JSON over stdin/stdout: the host
//! sends [`Request`]s, the worker replies with [`Message`]s (progress,
//! heartbeat, result). Untrusted PDF parsing and the OCR engine run only
//! inside workers, so a crash or hang there never takes down the UI; the
//! host enforces a per-request timeout with grace-period escalation.

pub mod host;
pub mod jobs;
pub mod protocol;

pub use host::{Worker, WorkerConfig};
pub use protocol::{Message, Request, RequestKind, Response};

/// Entry point for the `--worker` mode. Returns the process exit code.
pub fn run_stdio_worker() -> i32 {
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    let mut ctx = jobs::Context::default();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&Message::Error {
                        id: 0,
                        error: format!("bad request: {e}")
                    })
                    .unwrap()
                );
                let _ = stdout.flush();
                continue;
            }
        };
        if matches!(req.kind, RequestKind::Shutdown) {
            let _ = writeln!(
                stdout,
                "{}",
                serde_json::to_string(&Message::Result {
                    id: req.id,
                    result: Response::Ok
                })
                .unwrap()
            );
            let _ = stdout.flush();
            return 0;
        }
        let id = req.id;
        let mut emit = |m: Message| {
            // A message that cannot be serialized must not kill the worker;
            // report it as an error for the same request instead.
            let line = serde_json::to_string(&m).unwrap_or_else(|e| {
                serde_json::to_string(&Message::Error {
                    id: m.id(),
                    error: format!("serialize reply: {e}"),
                })
                .expect("error message serializes")
            });
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        };
        match jobs::handle(&mut ctx, req, &mut emit) {
            Ok(resp) => emit(Message::Result { id, result: resp }),
            Err(e) => emit(Message::Error {
                id,
                error: e.to_string(),
            }),
        }
    }
    0
}
