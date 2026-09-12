//! Preview rendering (IMG-01, D-14, UX-03).
//!
//! Pages are rendered by PDFium inside a dedicated worker process at one of a
//! few quantized scales and cached as lossless WebP in the project's cache
//! directory. The webview fetches them through the `sbwb-render` protocol:
//! `sbwb-render://localhost/p/<index>?s=<scale>` (on Windows WebView2 maps
//! this to `http://sbwb-render.localhost/...`). The cache is bounded by
//! total size with least-recently-used eviction (NFR-04).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use sbwb_core::{PageIndex, Result, SbwbError};
use sbwb_worker::{RequestKind, Response, Worker};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Scales (bitmap pixels per PDF point) the UI may request. Anything else
/// is refused so the cache stays bounded. 1.0 = 72 DPI, 4.1667 = 300 DPI.
pub const SCALES: [f64; 10] = [0.2, 0.35, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0];
/// Bitmap dimension cap; at scale 6 a 14-inch page is ~6 000 px.
const MAX_DIMENSION: u32 = 8192;
/// Default cache cap (2 GB). Evicted by age when exceeded.
pub const DEFAULT_CACHE_CAP: u64 = 2 * 1024 * 1024 * 1024;

fn scale_is_allowed(s: f64) -> bool {
    SCALES.iter().any(|a| (a - s).abs() < 1e-6)
}

pub struct RenderCache {
    dir: PathBuf,
    cap_bytes: u64,
    writes_since_check: Mutex<u32>,
}

impl RenderCache {
    pub fn new(project_cache_dir: &Path, cap_bytes: u64) -> Result<Self> {
        let dir = project_cache_dir.join("renders");
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            cap_bytes,
            writes_since_check: Mutex::new(0),
        })
    }

    pub fn path_for(&self, page: PageIndex, scale: f64) -> PathBuf {
        self.dir.join(format!("p{:05}_s{:.3}.webp", page.0, scale))
    }

    /// Return the cached file, rendering it through `worker` if missing.
    pub fn ensure(
        &self,
        worker: &mut Worker,
        source: &Path,
        page: PageIndex,
        scale: f64,
    ) -> Result<PathBuf> {
        let path = self.path_for(page, scale);
        if path.is_file() {
            // Touch so LRU eviction sees recent use.
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(SystemTime::now()));
            return Ok(path);
        }
        let resp = worker.call(
            RequestKind::RenderPage {
                path: source.to_path_buf(),
                password: None,
                page,
                scale,
                crop: None,
                max_dimension: MAX_DIMENSION,
                out: path.clone(),
            },
            |_| {},
        )?;
        match resp {
            Response::Rendered { .. } => {}
            other => {
                return Err(SbwbError::other(format!(
                    "unexpected render reply {other:?}"
                )))
            }
        }
        self.after_write()?;
        Ok(path)
    }

    fn after_write(&self) -> Result<()> {
        let mut n = self
            .writes_since_check
            .lock()
            .map_err(|_| SbwbError::other("cache mutex poisoned"))?;
        *n += 1;
        if *n >= 25 {
            *n = 0;
            drop(n);
            self.evict_to_cap()?;
        }
        Ok(())
    }

    /// Delete oldest files until the directory fits the cap.
    pub fn evict_to_cap(&self) -> Result<u64> {
        let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        let mut total = 0u64;
        for e in std::fs::read_dir(&self.dir)? {
            let e = e?;
            let md = e.metadata()?;
            if !md.is_file() {
                continue;
            }
            total += md.len();
            entries.push((
                md.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                md.len(),
                e.path(),
            ));
        }
        let mut freed = 0u64;
        if total > self.cap_bytes {
            entries.sort_by_key(|(t, _, _)| *t);
            for (_, len, path) in entries {
                if total <= self.cap_bytes {
                    break;
                }
                if std::fs::remove_file(&path).is_ok() {
                    total -= len;
                    freed += len;
                }
            }
        }
        Ok(freed)
    }
}

/// Parse `/p/<index>` and `s=<scale>` from a protocol request.
fn parse(uri: &http::Uri) -> std::result::Result<(PageIndex, f64), String> {
    let path = uri.path();
    let index = path
        .strip_prefix("/p/")
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| format!("bad path {path}"))?;
    let scale = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("s=")))
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| "missing scale".to_string())?;
    if !scale_is_allowed(scale) {
        return Err(format!(
            "scale {scale} is not one of the allowed preview scales"
        ));
    }
    Ok((PageIndex(index), scale))
}

fn response(status: u16, content_type: &str, body: Vec<u8>) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header(
            "Cache-Control",
            if status == 200 {
                "private, max-age=31536000, immutable"
            } else {
                "no-store"
            },
        )
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .expect("static response")
}

/// Serve one preview request. Runs on a pool thread.
pub fn handle(app: &AppHandle, request: &http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    let (page, scale) = match parse(request.uri()) {
        Ok(v) => v,
        Err(e) => return response(400, "text/plain", e.into_bytes()),
    };
    let state = app.state::<AppState>();
    let (source, cache) = {
        let guard = match state.project.lock() {
            Ok(g) => g,
            Err(_) => return response(500, "text/plain", b"state poisoned".to_vec()),
        };
        match guard.as_ref() {
            Some(p) => (p.source_path.clone(), p.render_cache.clone()),
            None => return response(404, "text/plain", b"no open book".to_vec()),
        }
    };
    let result = state.with_render_worker(|w| cache.ensure(w, &source, page, scale));
    match result {
        Ok(path) => match std::fs::read(&path) {
            Ok(bytes) => response(200, "image/webp", bytes),
            Err(e) => response(500, "text/plain", format!("read render: {e}").into_bytes()),
        },
        Err(SbwbError::NotFound(m)) => response(404, "text/plain", m.into_bytes()),
        Err(SbwbError::ResourceLimit(m)) => response(413, "text/plain", m.into_bytes()),
        Err(e) => {
            tracing::warn!("render {page} @ {scale}: {e}");
            response(500, "text/plain", e.to_string().into_bytes())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests_and_rejects_odd_scales() {
        let uri: http::Uri = "http://sbwb-render.localhost/p/12?s=0.5".parse().unwrap();
        assert_eq!(parse(&uri).unwrap(), (PageIndex(12), 0.5));
        let bad: http::Uri = "sbwb-render://localhost/p/12?s=0.51".parse().unwrap();
        assert!(parse(&bad).is_err());
        let nopage: http::Uri = "sbwb-render://localhost/x/12?s=0.5".parse().unwrap();
        assert!(parse(&nopage).is_err());
    }

    #[test]
    fn evicts_oldest_until_under_cap() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RenderCache::new(dir.path(), 3_000).unwrap();
        for (i, age) in [(0u32, 300u64), (1, 200), (2, 100), (3, 0)] {
            let p = cache.path_for(PageIndex(i), 1.0);
            std::fs::write(&p, vec![0u8; 1_000]).unwrap();
            let t = SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        let freed = cache.evict_to_cap().unwrap();
        assert_eq!(freed, 1_000);
        assert!(
            !cache.path_for(PageIndex(0), 1.0).exists(),
            "oldest should go first"
        );
        assert!(cache.path_for(PageIndex(3), 1.0).exists());
    }
}
