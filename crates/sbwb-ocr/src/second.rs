//! Optional second engine (OCR-02, M8.3): `ocrs` (pure Rust, RTen models)
//! recognises text lines. It has no per-word confidence, so its output is
//! line-level evidence only; line scores are never presented as word scores.

use std::path::{Path, PathBuf};

use sbwb_core::{Rect, Result, SbwbError, Transform};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecondLine {
    pub text: String,
    /// In page points.
    pub bbox: Rect,
    /// Word boxes in page points, in reading order within the line.
    pub words: Vec<(String, Rect)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecondOutput {
    pub engine: String,
    pub lines: Vec<SecondLine>,
}

pub fn model_paths(models_root: &Path) -> (PathBuf, PathBuf) {
    (
        models_root.join("ocrs/text-detection.rten"),
        models_root.join("ocrs/text-recognition.rten"),
    )
}

/// Whether this build carries the second engine at all.
pub fn compiled() -> bool {
    cfg!(feature = "ocrs")
}

pub fn available(models_root: &Path) -> bool {
    let (d, r) = model_paths(models_root);
    d.is_file() && r.is_file()
}

#[cfg(feature = "ocrs")]
pub fn recognize(
    models_root: &Path,
    image: &image::RgbImage,
    transform: &Transform,
) -> Result<SecondOutput> {
    use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
    let (det, rec) = model_paths(models_root);
    if !det.is_file() || !rec.is_file() {
        return Err(SbwbError::NotFound(
            "ocrs models are not installed (models/ocrs)".into(),
        ));
    }
    let detection = rten::Model::load_file(&det)
        .map_err(|e| SbwbError::other(format!("ocrs detection model: {e}")))?;
    let recognition = rten::Model::load_file(&rec)
        .map_err(|e| SbwbError::other(format!("ocrs recognition model: {e}")))?;
    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection),
        recognition_model: Some(recognition),
        ..Default::default()
    })
    .map_err(|e| SbwbError::other(format!("ocrs: {e}")))?;
    let src = ImageSource::from_bytes(image.as_raw(), image.dimensions())
        .map_err(|e| SbwbError::other(format!("ocrs input: {e}")))?;
    let input = engine
        .prepare_input(src)
        .map_err(|e| SbwbError::other(format!("ocrs prepare: {e}")))?;
    let words = engine
        .detect_words(&input)
        .map_err(|e| SbwbError::other(format!("ocrs detect: {e}")))?;
    let lines = engine.find_text_lines(&input, &words);
    let texts = engine
        .recognize_text(&input, &lines)
        .map_err(|e| SbwbError::other(format!("ocrs recognise: {e}")))?;
    let to_pt = |r: rten_imageproc::Rect<i32>| -> Rect {
        transform.to_points(&Rect::new(
            r.left() as f64,
            r.top() as f64,
            r.width().max(1) as f64,
            r.height().max(1) as f64,
        ))
    };
    let mut out = Vec::new();
    for line in texts.into_iter().flatten() {
        let text = line.to_string();
        if text.trim().is_empty() {
            continue;
        }
        let bbox = to_pt(line.bounding_rect());
        let words = line
            .words()
            .map(|w| (w.to_string(), to_pt(w.bounding_rect())))
            .filter(|(t, _)| !t.trim().is_empty())
            .collect();
        out.push(SecondLine { text, bbox, words });
    }
    Ok(SecondOutput {
        engine: "ocrs 0.13".to_string(),
        lines: out,
    })
}

#[cfg(not(feature = "ocrs"))]
pub fn recognize(
    _models_root: &Path,
    _image: &image::RgbImage,
    _transform: &Transform,
) -> Result<SecondOutput> {
    Err(SbwbError::other("this build has no second engine"))
}
