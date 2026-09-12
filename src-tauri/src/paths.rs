//! Locations of bundled native resources (PDFium, tessdata).
//!
//! In a packaged build they live under the Tauri resource directory. In
//! development they are read from the repository's `third_party/` and
//! `models/` folders (see docs/dev-setup.md). `SBWB_PDFIUM_DIR` and
//! `SBWB_TESSDATA_DIR` override both.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

#[derive(Debug, Clone)]
pub struct Resources {
    pub pdfium_dir: PathBuf,
    pub tessdata_dir: PathBuf,
}

impl Resources {
    pub fn locate(app: &AppHandle) -> Self {
        let resource_dir = app.path().resource_dir().ok();
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");

        let pdfium_dir = std::env::var_os("SBWB_PDFIUM_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                resource_dir
                    .as_ref()
                    .map(|r| r.join("pdfium"))
                    .filter(|p| p.join(pdfium_lib_name()).exists())
            })
            .unwrap_or_else(|| repo_root.join("third_party/pdfium/bin"));

        let tessdata_dir = std::env::var_os("SBWB_TESSDATA_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                resource_dir
                    .as_ref()
                    .map(|r| r.join("tessdata"))
                    .filter(|p| p.join("best/eng.traineddata").exists())
            })
            .unwrap_or_else(|| repo_root.join("models"));

        Self {
            pdfium_dir: simplify(pdfium_dir),
            tessdata_dir: simplify(tessdata_dir),
        }
    }
}

pub fn pdfium_lib_name() -> &'static str {
    if cfg!(windows) {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

/// Strip the Windows verbatim prefix (`\\?\C:\...`). Native libraries such
/// as Tesseract join forward-slash file names onto the directory, which the
/// verbatim form rejects.
pub fn simplify(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefix() {
        assert_eq!(
            simplify(PathBuf::from(r"\\?\C:\x\y")),
            PathBuf::from(r"C:\x\y")
        );
        assert_eq!(
            simplify(PathBuf::from(r"\\?\UNC\srv\share")),
            PathBuf::from(r"\\srv\share")
        );
        assert_eq!(
            simplify(PathBuf::from(r"C:\plain")),
            PathBuf::from(r"C:\plain")
        );
    }
}
