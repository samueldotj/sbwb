//! Model pack catalog and verification (OCR-01, M3.5).
//!
//! Packs live under `<tessdata_root>/<pack>/<lang>.traineddata`. The catalog
//! records the SHA-256 of every known file so an installed pack can be
//! verified and a user-supplied file can be installed from disk without a
//! network. Downloads are user-initiated and left to the caller (the app
//! bundles both English packs, D-22).

use std::path::{Path, PathBuf};

use sbwb_core::{Result, SbwbError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ModelPack;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub pack: ModelPack,
    pub label: String,
    pub language: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
    pub license: String,
    pub url: String,
    /// Whether the pack supports page-level segmentation and line
    /// recognition (Tesseract does both).
    pub detection: bool,
    pub recognition: bool,
}

pub fn catalog() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            pack: ModelPack::EngBest,
            label: "English (tessdata_best)".into(),
            language: "eng".into(),
            file: "best/eng.traineddata".into(),
            sha256: "8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba".into(),
            size: 15_400_601,
            license: "Apache-2.0".into(),
            url: "https://github.com/tesseract-ocr/tessdata_best/raw/main/eng.traineddata".into(),
            detection: true,
            recognition: true,
        },
        CatalogEntry {
            pack: ModelPack::EngFast,
            label: "English (fast)".into(),
            language: "eng".into(),
            file: "fast/eng.traineddata".into(),
            sha256: "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2".into(),
            size: 4_113_088,
            license: "Apache-2.0".into(),
            url: "https://github.com/tesseract-ocr/tessdata_fast/raw/main/eng.traineddata".into(),
            detection: true,
            recognition: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackStatus {
    Missing,
    Verified,
    Corrupt { actual_sha256: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackReport {
    pub entry: CatalogEntry,
    pub path: PathBuf,
    pub status: PackStatus,
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut hasher = Sha256::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Status of every catalog pack under `root`.
pub fn report(root: &Path) -> Vec<PackReport> {
    catalog()
        .into_iter()
        .map(|entry| {
            let path = root.join(&entry.file);
            let status = if !path.is_file() {
                PackStatus::Missing
            } else {
                match sha256_file(&path) {
                    Ok(h) if h == entry.sha256 => PackStatus::Verified,
                    Ok(h) => PackStatus::Corrupt { actual_sha256: h },
                    Err(_) => PackStatus::Missing,
                }
            };
            PackReport {
                entry,
                path,
                status,
            }
        })
        .collect()
}

/// Install a pack from a file the user chose (verified before copying).
pub fn install_from_file(root: &Path, pack: ModelPack, source: &Path) -> Result<PackReport> {
    let entry = catalog()
        .into_iter()
        .find(|e| e.pack == pack)
        .ok_or_else(|| SbwbError::NotFound(format!("{pack:?} is not in the catalog")))?;
    let actual = sha256_file(source)?;
    if actual != entry.sha256 {
        return Err(SbwbError::Integrity(format!(
            "{} does not match the catalog checksum for {} (got {}…)",
            source.display(),
            entry.label,
            &actual[..12]
        )));
    }
    let dest = root.join(&entry.file);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("part");
    std::fs::copy(source, &tmp)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(PackReport {
        entry,
        path: dest,
        status: PackStatus::Verified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_bundled_models() {
        let root = crate::default_tessdata_root();
        if !root.join("fast/eng.traineddata").exists() {
            return;
        }
        let r = report(&root);
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|p| p.status == PackStatus::Verified), "{r:?}");
    }

    #[test]
    fn rejects_wrong_checksum_on_install() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("eng.traineddata");
        std::fs::write(&bogus, b"not a model").unwrap();
        let err = install_from_file(dir.path(), ModelPack::EngFast, &bogus).unwrap_err();
        assert!(matches!(err, SbwbError::Integrity(_)));
        let missing = report(dir.path());
        assert!(missing.iter().all(|p| p.status == PackStatus::Missing));
    }
}
