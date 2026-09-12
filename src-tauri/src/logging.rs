//! Rotated file logging with path redaction (M0.8, NFR-11).
//!
//! The user's home directory is replaced by `~` in every line written to the
//! log file so diagnostics bundles do not carry the account name.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

struct Redacting<W> {
    inner: W,
    home: Option<String>,
}

impl<W: Write> Write for Redacting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let (Some(home), Ok(text)) = (&self.home, std::str::from_utf8(buf)) {
            if text.contains(home.as_str()) {
                let redacted = text.replace(home.as_str(), "~");
                self.inner.write_all(redacted.as_bytes())?;
                return Ok(buf.len());
            }
        }
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Shares one appender across writers; the mutex keeps lines whole.
struct SharedAppender(Arc<Mutex<tracing_appender::rolling::RollingFileAppender>>);

impl Write for SharedAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("log mutex poisoned"))?
            .write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("log mutex poisoned"))?
            .flush()
    }
}

struct RedactingMaker {
    appender: Arc<Mutex<tracing_appender::rolling::RollingFileAppender>>,
    home: Option<String>,
}

impl<'a> fmt::MakeWriter<'a> for RedactingMaker {
    type Writer = Redacting<SharedAppender>;
    fn make_writer(&'a self) -> Self::Writer {
        Redacting {
            inner: SharedAppender(Arc::clone(&self.appender)),
            home: self.home.clone(),
        }
    }
}

fn home_dir() -> Option<String> {
    std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .map(|h| h.trim_end_matches(['\\', '/']).to_string())
        .filter(|h| !h.is_empty())
}

pub fn init(log_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(log_dir)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("sbwb")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)?;
    let maker = RedactingMaker {
        appender: Arc::new(Mutex::new(appender)),
        home: home_dir(),
    };

    let filter = EnvFilter::try_from_env("SBWB_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(maker);
    let registry = tracing_subscriber::registry().with(filter).with(file_layer);

    #[cfg(debug_assertions)]
    let registry = registry.with(fmt::layer().with_target(false).with_writer(io::stderr));

    registry
        .try_init()
        .map_err(|e| anyhow::anyhow!("logging init: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_home_directory() {
        let mut buf = Vec::new();
        {
            let mut w = Redacting {
                inner: &mut buf,
                home: Some("C:\\Users\\alice".into()),
            };
            w.write_all(b"opened C:\\Users\\alice\\Documents\\book.sbwb\n")
                .unwrap();
        }
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "opened ~\\Documents\\book.sbwb\n"
        );
    }
}
