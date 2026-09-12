//! Structural inspection with `lopdf` (PRJ-01). Runs in the worker process
//! for untrusted inputs (NFR-11).

use std::io::Read;
use std::path::Path;

use sbwb_core::{Rect, Result, SbwbError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfPageInfo {
    /// Media box size in points.
    pub width_pt: f64,
    pub height_pt: f64,
    /// Largest embedded image on the page, if any (width, height in pixels).
    pub image_px: Option<(u32, u32)>,
    /// Approximate DPI of that image relative to the page width.
    pub image_dpi: Option<f64>,
    /// Whether the page has an extractable text layer.
    pub has_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfInfo {
    pub page_count: u32,
    pub encrypted: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    pub producer: Option<String>,
    pub file_size: u64,
    /// BLAKE3 hex digest of the whole file (PRJ-01).
    pub blake3: String,
    /// Info for the first few pages; the full per-page table is built lazily.
    pub sample_pages: Vec<PdfPageInfo>,
    pub pdf_version: String,
}

pub fn blake3_file(path: &Path) -> Result<String> {
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

fn text_of(doc: &lopdf::Document, key: &[u8]) -> Option<String> {
    let info = doc.trailer.get(b"Info").ok()?;
    let info = match info {
        lopdf::Object::Reference(r) => doc.get_object(*r).ok()?,
        o => o,
    };
    let dict = info.as_dict().ok()?;
    let obj = dict.get(key).ok()?;
    let obj = match obj {
        lopdf::Object::Reference(r) => doc.get_object(*r).ok()?,
        o => o,
    };
    let s = obj.as_str().ok()?;
    Some(decode_pdf_text(s))
}

/// Decode a PDF text string: UTF-16BE with BOM, otherwise PDFDocEncoding
/// treated as Latin-1 (adequate for Info dictionary fields).
fn decode_pdf_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn page_info(doc: &lopdf::Document, page_id: lopdf::ObjectId, page_num: u32) -> PdfPageInfo {
    let (w, h) = doc
        .get_object(page_id)
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"MediaBox").ok().or_else(|| d.get(b"CropBox").ok()))
        .and_then(|mb| match mb {
            lopdf::Object::Reference(r) => doc.get_object(*r).ok(),
            o => Some(o),
        })
        .and_then(|o| o.as_array().ok())
        .filter(|a| a.len() == 4)
        .map(|a| {
            let f = |i: usize| {
                a[i].as_float()
                    .or_else(|_| a[i].as_i64().map(|v| v as f32))
                    .unwrap_or(0.0) as f64
            };
            let r = Rect::from_corners(f(0), f(1), f(2), f(3));
            (r.w, r.h)
        })
        .unwrap_or((612.0, 792.0));

    let mut image_px: Option<(u32, u32)> = None;
    if let Ok(images) = doc.get_page_images(page_id) {
        for im in images {
            let (iw, ih) = (im.width.max(0) as u32, im.height.max(0) as u32);
            if image_px.is_none_or(|(a, b)| (iw as u64 * ih as u64) > (a as u64 * b as u64)) {
                image_px = Some((iw, ih));
            }
        }
    }
    let image_dpi = image_px
        .filter(|_| w > 0.0)
        .map(|(iw, _)| iw as f64 * sbwb_core::POINTS_PER_INCH / w);
    let has_text = doc
        .extract_text(&[page_num])
        .map(|t| t.chars().any(|c| c.is_alphanumeric()))
        .unwrap_or(false);

    PdfPageInfo {
        width_pt: w,
        height_pt: h,
        image_px,
        image_dpi,
        has_text,
    }
}

/// Inspect a PDF file. `sample_pages` limits how many pages get per-page
/// details (image sizes need object walks; 529 pages take a few seconds).
pub fn inspect_file(path: &Path, sample_pages: u32) -> Result<PdfInfo> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(SbwbError::invalid("not a file"));
    }
    let blake3 = blake3_file(path)?;
    let doc = lopdf::Document::load(path)
        .map_err(|e| SbwbError::invalid(format!("cannot read PDF: {e}")))?;
    let encrypted = doc.is_encrypted();
    let pages = doc.get_pages();
    let page_count = pages.len() as u32;
    if page_count == 0 && !encrypted {
        return Err(SbwbError::invalid("PDF has no pages"));
    }
    let mut sample = Vec::new();
    if !encrypted {
        for (num, id) in pages.iter().take(sample_pages as usize) {
            sample.push(page_info(&doc, *id, *num));
        }
    }
    Ok(PdfInfo {
        page_count,
        encrypted,
        title: text_of(&doc, b"Title"),
        author: text_of(&doc, b"Author"),
        producer: text_of(&doc, b"Producer"),
        file_size: meta.len(),
        blake3,
        sample_pages: sample,
        pdf_version: doc.version.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
    }

    #[test]
    fn inspects_fixture() {
        let info = inspect_file(&fixture(), 3).unwrap();
        assert_eq!(info.page_count, 110);
        assert!(!info.encrypted);
        assert_eq!(info.author.as_deref(), Some("James Hough"));
        assert_eq!(info.sample_pages.len(), 3);
        let p = &info.sample_pages[0];
        assert!(p.width_pt > 300.0 && p.width_pt < 400.0);
        assert_eq!(info.blake3.len(), 64);
    }

    #[test]
    fn rejects_non_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.pdf");
        std::fs::write(&p, b"not a pdf").unwrap();
        assert!(matches!(
            inspect_file(&p, 1),
            Err(SbwbError::InvalidInput(_))
        ));
    }
}
