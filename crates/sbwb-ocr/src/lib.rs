//! Tesseract 5 adapter (OCR-01, PROV-01).
//!
//! Produces [`OcrWord`]s with pixel boxes converted to page points through
//! the render [`Transform`], the engine's native word confidence (0-100),
//! and the raw TSV/hOCR kept as evidence. Line and block structure from
//! Tesseract's own segmentation is retained as extra layout evidence (D-11).

pub mod tsv;

use std::path::Path;

use sbwb_core::{Rect, Result, SbwbError, Transform};
use serde::{Deserialize, Serialize};

pub use tsv::{parse_tsv, OcrBlock, OcrLine, OcrWord, TsvPage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPack {
    /// `tessdata_fast/eng`
    EngFast,
    /// `tessdata_best/eng` (default, D-22)
    EngBest,
}

impl ModelPack {
    pub fn subdir(self) -> &'static str {
        match self {
            ModelPack::EngFast => "fast",
            ModelPack::EngBest => "best",
        }
    }
    pub fn language(self) -> &'static str {
        "eng"
    }
    pub fn label(self) -> &'static str {
        match self {
            ModelPack::EngFast => "Tesseract eng (fast)",
            ModelPack::EngBest => "Tesseract eng (tessdata_best)",
        }
    }
}

/// Page segmentation modes we expose (subset of Tesseract's PSM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Segmentation {
    /// PSM 1: automatic with OSD.
    AutoOsd,
    /// PSM 3: fully automatic, no OSD (default for pages).
    Auto,
    /// PSM 6: single uniform block (used for region crops).
    SingleBlock,
    /// PSM 7: single text line (marginalia crops).
    SingleLine,
}

impl Segmentation {
    fn psm(self) -> &'static str {
        match self {
            Segmentation::AutoOsd => "1",
            Segmentation::Auto => "3",
            Segmentation::SingleBlock => "6",
            Segmentation::SingleLine => "7",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrSettings {
    pub model: ModelPack,
    pub segmentation: Segmentation,
    /// Source resolution hint passed to the engine.
    pub dpi: u32,
}

impl Default for OcrSettings {
    fn default() -> Self {
        Self {
            model: ModelPack::EngBest,
            segmentation: Segmentation::Auto,
            dpi: sbwb_core::RECOGNITION_DPI,
        }
    }
}

/// Result of recognizing one bitmap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrOutput {
    pub words: Vec<OcrWord>,
    pub lines: Vec<OcrLine>,
    pub blocks: Vec<OcrBlock>,
    /// Raw TSV as produced by the engine (evidence).
    pub tsv: String,
    pub engine: String,
    pub model: ModelPack,
    pub settings: OcrSettings,
    /// Mean word confidence, 0-100, over words with text.
    pub mean_confidence: f32,
}

pub struct Engine {
    tessdata_root: std::path::PathBuf,
}

impl Engine {
    /// `tessdata_root` contains `best/eng.traineddata` and `fast/eng.traineddata`.
    pub fn new(tessdata_root: &Path) -> Self {
        Self {
            tessdata_root: tessdata_root.to_path_buf(),
        }
    }

    pub fn version() -> String {
        // SAFETY: TessVersion returns a pointer to a static NUL-terminated string.
        unsafe { std::ffi::CStr::from_ptr(tesseract_sys::TessVersion()) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn model_available(&self, model: ModelPack) -> bool {
        self.tessdata_root
            .join(model.subdir())
            .join(format!("{}.traineddata", model.language()))
            .exists()
    }

    /// Recognize an RGB bitmap. `transform` maps bitmap pixels back to page
    /// points; pass the renderer's transform so word boxes land in page space.
    pub fn recognize(
        &self,
        image: &image::RgbImage,
        transform: &Transform,
        settings: &OcrSettings,
    ) -> Result<OcrOutput> {
        let datapath = self.tessdata_root.join(settings.model.subdir());
        if !self.model_available(settings.model) {
            return Err(SbwbError::NotFound(format!(
                "model pack {:?} at {}",
                settings.model,
                datapath.display()
            )));
        }
        let datapath_s = datapath.to_string_lossy().to_string();
        let (w, h) = (image.width() as i32, image.height() as i32);
        let raw = image.as_raw();
        let tess = tesseract::Tesseract::new(Some(&datapath_s), Some(settings.model.language()))
            .map_err(|e| SbwbError::other(format!("tesseract init: {e}")))?
            .set_variable("tessedit_pageseg_mode", settings.segmentation.psm())
            .map_err(|e| SbwbError::other(format!("tesseract psm: {e}")))?
            .set_frame(raw, w, h, 3, w * 3)
            .map_err(|e| SbwbError::other(format!("tesseract frame: {e}")))?
            .set_source_resolution(settings.dpi as i32);
        let mut tess = tess
            .recognize()
            .map_err(|e| SbwbError::other(format!("tesseract recognize: {e}")))?;
        let tsv = tess
            .get_tsv_text(0)
            .map_err(|e| SbwbError::other(format!("tesseract tsv: {e}")))?;
        let page: TsvPage = parse_tsv(&tsv, transform);
        let conf: Vec<f32> = page
            .words
            .iter()
            .filter(|w| !w.text.is_empty())
            .map(|w| w.confidence)
            .collect();
        let mean = if conf.is_empty() {
            0.0
        } else {
            conf.iter().sum::<f32>() / conf.len() as f32
        };
        Ok(OcrOutput {
            words: page.words,
            lines: page.lines,
            blocks: page.blocks,
            tsv,
            engine: format!("tesseract {}", Self::version()),
            model: settings.model,
            settings: settings.clone(),
            mean_confidence: mean,
        })
    }

    /// Recognize a rectangular sub-area of a bitmap (region OCR, OCR-02).
    pub fn recognize_crop(
        &self,
        image: &image::RgbImage,
        transform: &Transform,
        crop_px: Rect,
        settings: &OcrSettings,
    ) -> Result<OcrOutput> {
        let x = crop_px.x.max(0.0) as u32;
        let y = crop_px.y.max(0.0) as u32;
        let w = (crop_px.w as u32)
            .min(image.width().saturating_sub(x))
            .max(1);
        let h = (crop_px.h as u32)
            .min(image.height().saturating_sub(y))
            .max(1);
        let sub = image::imageops::crop_imm(image, x, y, w, h).to_image();
        let origin_pt = transform.to_points(&Rect::new(x as f64, y as f64, 0.0, 0.0));
        let t = Transform {
            sx: transform.sx,
            sy: transform.sy,
            origin: sbwb_core::Point {
                x: origin_pt.x,
                y: origin_pt.y,
            },
        };
        self.recognize(&sub, &t, settings)
    }
}

pub fn default_tessdata_root() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("SBWB_TESSDATA_DIR") {
        return d.into();
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::PageIndex;

    #[test]
    fn recognizes_fixture_page() {
        let tessdata = default_tessdata_root();
        let pdfium = sbwb_pdf::default_pdfium_dir();
        if !tessdata.join("fast/eng.traineddata").exists() || !pdfium.join("pdfium.dll").exists() {
            eprintln!("skipping: models or pdfium missing");
            return;
        }
        let pdf = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf");
        let r = sbwb_pdf::Renderer::new(&pdfium).unwrap();
        let rendered = r
            .render(
                &pdf,
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(60), 300),
            )
            .unwrap();
        let engine = Engine::new(&tessdata);
        let settings = OcrSettings {
            model: ModelPack::EngFast,
            ..Default::default()
        };
        let out = engine
            .recognize(&rendered.image, &rendered.transform, &settings)
            .unwrap();
        let text: String = out
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "mean conf {:.1}, {} words: {}",
            out.mean_confidence,
            out.words.len(),
            &text[..text.len().min(200)]
        );
        assert!(
            out.words.len() > 100,
            "expected a full page of words, got {}",
            out.words.len()
        );
        assert!(
            text.contains("India") || text.contains("Egypt"),
            "text: {text}"
        );
        assert!(out.mean_confidence > 60.0);
        // boxes are in page points inside the page
        let (pw, ph) = rendered.page_size;
        assert!(out.words.iter().all(|w| w.bbox.x >= -1.0
            && w.bbox.right() <= pw + 1.0
            && w.bbox.bottom() <= ph + 1.0));
    }
}
