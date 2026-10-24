//! App updates from GitHub releases: check the latest release, download
//! its installer, verify it against the release's SHA256SUMS.txt, then run
//! it and quit. Nothing goes online unless the user asks, or has turned on
//! the check at startup (NFR-11).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, CmdResult, CommandError};

pub const EVENT_UPDATE: &str = "update:event";

const LATEST_URL: &str = "https://api.github.com/repos/samueldotj/sbwb/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/samueldotj/sbwb/releases/";
const SUMS_NAME: &str = "SHA256SUMS.txt";

#[derive(Default)]
pub struct UpdateSlot {
    pub running: Mutex<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateEvent {
    Progress {
        done: u64,
        total: u64,
    },
    /// Verified; `launching` is false when this build cannot run the
    /// installer (a development build), and the file is left at `path`.
    Downloaded {
        path: String,
        launching: bool,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub published_at: Option<String>,
    /// The installer this install would use, absent when the release has none.
    pub installer: Option<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// How this copy was installed, which decides the installer to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallKind {
    Nsis,
    Msi,
}

fn install_kind() -> InstallKind {
    // The NSIS installer writes uninstall.exe beside the app; MSI does not.
    let nsis = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("uninstall.exe").is_file()))
        .unwrap_or(false);
    if nsis {
        InstallKind::Nsis
    } else {
        InstallKind::Msi
    }
}

/// `v0.3.0` / `0.3.0` → (0, 3, 0). Pre-release suffixes are ignored.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn pick_installer(assets: &[Asset], kind: InstallKind) -> Option<&Asset> {
    let suffix = match kind {
        InstallKind::Nsis => "-setup.exe",
        InstallKind::Msi => ".msi",
    };
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n.ends_with(suffix) && n.contains("x64") && safe_file_name(&a.name)
    })
}

/// Asset names become file names in the temp folder; refuse anything that
/// could leave it.
fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The lower-case hex SHA-256 listed for `name` in a `sha256sum` file.
fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn latest_url() -> String {
    std::env::var("SBWB_UPDATE_URL").unwrap_or_else(|_| LATEST_URL.to_string())
}

fn client(timeout: Option<std::time::Duration>) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("sbwb/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())
}

fn fetch_latest() -> Result<Release, String> {
    let resp = client(Some(std::time::Duration::from_secs(30)))?
        .get(latest_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("could not reach GitHub: {e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("no release has been published yet".into());
    }
    if !status.is_success() {
        return Err(format!("GitHub answered {status}"));
    }
    resp.json::<Release>()
        .map_err(|e| format!("unexpected answer from GitHub: {e}"))
}

fn info_from(release: Release) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    UpdateInfo {
        newer: is_newer(&latest, &current),
        installer: pick_installer(&release.assets, install_kind()).cloned(),
        name: release.name.unwrap_or_else(|| format!("SBWB {latest}")),
        notes: release.body.unwrap_or_default(),
        url: release.html_url,
        published_at: release.published_at,
        current,
        latest,
    }
}

/// Ask GitHub for the latest release and compare it with this version.
#[tauri::command]
pub async fn update_check() -> CmdResult<UpdateInfo> {
    let release = tauri::async_runtime::spawn_blocking(fetch_latest)
        .await
        .map_err(|e| CommandError::new("other", e))?
        .map_err(|e| CommandError::new("network", e))?;
    let info = info_from(release);
    tracing::info!(current = %info.current, latest = %info.latest, newer = info.newer, "update check");
    Ok(info)
}

fn download(
    url: &str,
    dest: &Path,
    expected: &str,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let mut resp = client(None)?
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download failed: {e}"))?;
    let total = resp.content_length().unwrap_or(0);
    let part = dest.with_extension("part");
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last = 0u64;
    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .map_err(|e| format!("{}: {e}", part.display()))?;
        done += n as u64;
        if done - last >= 1 << 20 || done == total {
            last = done;
            progress(done, total);
        }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != expected {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "the downloaded installer does not match its published checksum (expected {}…, got {}…)",
            &expected[..12],
            &got[..12]
        ));
    }
    std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(())
}

/// Fetch the latest installer and its checksum; returns the verified file.
fn fetch_installer(progress: &mut dyn FnMut(u64, u64)) -> Result<PathBuf, String> {
    let release = fetch_latest()?;
    let kind = install_kind();
    let asset = pick_installer(&release.assets, kind)
        .cloned()
        .ok_or("the latest release has no installer for this copy")?;
    let sums_url = release
        .assets
        .iter()
        .find(|a| a.name == SUMS_NAME)
        .map(|a| a.browser_download_url.clone())
        .ok_or("the latest release has no SHA256SUMS.txt to verify against")?;
    let sums = client(Some(std::time::Duration::from_secs(30)))?
        .get(&sums_url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("could not fetch {SUMS_NAME}: {e}"))?;
    let expected = checksum_for(&sums, &asset.name)
        .ok_or_else(|| format!("{SUMS_NAME} lists no checksum for {}", asset.name))?;
    let dir = std::env::temp_dir().join("sbwb-update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(&asset.name);
    download(&asset.browser_download_url, &dest, &expected, progress)?;
    tracing::info!(file = %dest.display(), sha256 = %expected, "update downloaded and verified");
    Ok(dest)
}

/// Close the open book cleanly before the installer replaces the app, so
/// the writer lock is released and nothing is mid-write (PRJ-04).
fn close_for_install(app: &AppHandle) {
    crate::commands::pipeline::stop_for_close(app);
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut guard) = state.project.lock() {
            if let Some(p) = guard.take() {
                let _ = p.store.close();
            }
        }
    }
}

#[cfg(windows)]
fn launch_installer(path: &Path) -> Result<(), String> {
    let mut cmd = match install_kind() {
        // Passive: progress only, closes the app if still running, and /R
        // starts the new version when it finishes.
        InstallKind::Nsis => {
            let mut c = std::process::Command::new(path);
            c.args(["/P", "/R", "/UPDATE"]);
            c
        }
        InstallKind::Msi => {
            let mut c = std::process::Command::new("msiexec");
            c.arg("/i").arg(path).arg("/passive");
            c
        }
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the installer: {e}"))
}

#[cfg(not(windows))]
fn launch_installer(_path: &Path) -> Result<(), String> {
    Err("installing updates is only supported on Windows".into())
}

/// Open the latest release's page in the browser. The address is re-read
/// from GitHub and must be on this repository, so the page cannot open
/// anything else.
#[tauri::command]
pub async fn update_open_page() -> CmdResult<()> {
    let release = tauri::async_runtime::spawn_blocking(fetch_latest)
        .await
        .map_err(|e| CommandError::new("other", e))?
        .map_err(|e| CommandError::new("network", e))?;
    if !release.html_url.starts_with(RELEASES_PAGE) {
        return Err(CommandError::new(
            "invalid_input",
            "the release page is not on the SBWB repository",
        ));
    }
    #[cfg(windows)]
    std::process::Command::new("explorer")
        .arg(&release.html_url)
        .spawn()
        .map_err(|e| CommandError::new("other", format!("open browser: {e}")))?;
    Ok(())
}

/// Download, verify and run the latest installer on a background thread;
/// progress and the outcome arrive as `update:event`. On success the app
/// quits so the installer can replace it.
#[tauri::command]
pub fn update_install(app: AppHandle) -> CmdResult<()> {
    let slot = app.state::<UpdateSlot>();
    {
        let mut running = slot
            .running
            .lock()
            .map_err(|_| CommandError::new("other", "update mutex poisoned"))?;
        if *running {
            return Err(CommandError::new(
                "conflict",
                "an update is already downloading",
            ));
        }
        *running = true;
    }
    let handle = app.clone();
    std::thread::Builder::new()
        .name("sbwb-update".into())
        .spawn(move || {
            let mut progress = |done: u64, total: u64| {
                let _ = handle.emit(EVENT_UPDATE, UpdateEvent::Progress { done, total });
            };
            let result = fetch_installer(&mut progress).and_then(|path| {
                // A development build has no installed copy to replace.
                let launching = !cfg!(debug_assertions);
                if launching {
                    close_for_install(&handle);
                    launch_installer(&path)?;
                }
                Ok((path, launching))
            });
            if let Some(slot) = handle.try_state::<UpdateSlot>() {
                if let Ok(mut r) = slot.running.lock() {
                    *r = false;
                }
            }
            match result {
                Ok((path, launching)) => {
                    let _ = handle.emit(
                        EVENT_UPDATE,
                        UpdateEvent::Downloaded {
                            path: path.to_string_lossy().into_owned(),
                            launching,
                        },
                    );
                    if launching {
                        tracing::info!("installer started; quitting for the update");
                        std::thread::sleep(std::time::Duration::from_millis(400));
                        handle.exit(0);
                    }
                }
                Err(message) => {
                    tracing::warn!("update failed: {message}");
                    let _ = handle.emit(EVENT_UPDATE, UpdateEvent::Failed { message });
                }
            }
        })
        .map_err(|e| CommandError::new("other", format!("update thread: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            size: 1,
            browser_download_url: format!("https://example.test/{name}"),
        }
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("v0.3.0"), Some((0, 3, 0)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.4.0-beta.1"), Some((0, 4, 0)));
        assert_eq!(parse_version("latest"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("nightly", "0.3.0"));
    }

    #[test]
    fn installer_matches_the_install_kind() {
        let assets = [
            asset("SBWB_0.4.0_x64_en-US.msi"),
            asset("SBWB_0.4.0_x64-setup.exe"),
            asset("SHA256SUMS.txt"),
        ];
        assert_eq!(
            pick_installer(&assets, InstallKind::Nsis).unwrap().name,
            "SBWB_0.4.0_x64-setup.exe"
        );
        assert_eq!(
            pick_installer(&assets, InstallKind::Msi).unwrap().name,
            "SBWB_0.4.0_x64_en-US.msi"
        );
        assert!(pick_installer(&[asset("..\\x64-setup.exe")], InstallKind::Nsis).is_none());
        assert!(pick_installer(&[asset("notes.md")], InstallKind::Nsis).is_none());
    }

    #[test]
    fn checksum_is_read_from_sha256sums() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  SBWB_0.4.0_x64-setup.exe\r\n{b} *SBWB_0.4.0_x64_en-US.msi\r\n");
        assert_eq!(checksum_for(&sums, "SBWB_0.4.0_x64-setup.exe"), Some(a));
        assert_eq!(
            checksum_for(&sums, "SBWB_0.4.0_x64_en-US.msi"),
            Some("b".repeat(64))
        );
        assert_eq!(checksum_for(&sums, "other.exe"), None);
        assert_eq!(checksum_for("abc  x.exe", "x.exe"), None);
    }
}
