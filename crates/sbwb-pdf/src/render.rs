//! PDFium rendering (IMG-01, D-14).
//!
//! A [`Renderer`] owns one PDFium binding and is not `Send`; the render
//! service keeps it on a dedicated thread. Renders return an 8-bit RGB
//! image plus the [`Transform`] from page points to bitmap pixels so callers
//! can map geometry both ways (PROV-01).

use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;
use sbwb_core::{PageIndex, Rect, Result, SbwbError, Transform};
use serde::{Deserialize, Serialize};

pub struct Renderer {
    pdfium: Pdfium,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub page: PageIndex,
    /// Pixels per point. 300 DPI = 300/72.
    pub scale: f64,
    /// Optional crop in page points; `None` renders the whole page.
    pub crop: Option<Rect>,
    /// Maximum bitmap dimension; larger requests fail with ResourceLimit
    /// instead of silently reducing scale (OCR-02, NFR-04).
    pub max_dimension: u32,
}

impl RenderRequest {
    pub fn page_at_dpi(page: PageIndex, dpi: u32) -> Self {
        Self {
            page,
            scale: dpi as f64 / sbwb_core::POINTS_PER_INCH,
            crop: None,
            max_dimension: 16_384,
        }
    }
}

pub struct Rendered {
    pub image: image::RgbImage,
    pub transform: Transform,
    /// Page size in points.
    pub page_size: (f64, f64),
}

impl Renderer {
    /// Bind PDFium from `dir` (containing `pdfium.dll` / `libpdfium.so`).
    pub fn new(dir: &Path) -> Result<Self> {
        let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
        // PDFium binds once per process (a global in pdfium-render); later
        // renderers in the same process reuse those bindings.
        let pdfium = match Pdfium::bind_to_library(&lib) {
            Ok(bindings) => Pdfium::new(bindings),
            Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
            Err(e) => match Pdfium::bind_to_system_library() {
                Ok(bindings) => Pdfium::new(bindings),
                Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
                Err(_) => {
                    return Err(SbwbError::other(format!(
                        "cannot load PDFium from {}: {e}",
                        dir.display()
                    )))
                }
            },
        };
        Ok(Self { pdfium })
    }

    pub fn page_count(&self, path: &Path, password: Option<&str>) -> Result<u32> {
        let doc = self.load(path, password)?;
        Ok(doc.pages().len() as u32)
    }

    pub fn page_size(
        &self,
        path: &Path,
        password: Option<&str>,
        page: PageIndex,
    ) -> Result<(f64, f64)> {
        let doc = self.load(path, password)?;
        let p = doc
            .pages()
            .get(page.0 as i32)
            .map_err(|e| SbwbError::NotFound(format!("{page}: {e}")))?;
        Ok((p.width().value as f64, p.height().value as f64))
    }

    pub fn render(
        &self,
        path: &Path,
        password: Option<&str>,
        req: &RenderRequest,
    ) -> Result<Rendered> {
        let doc = self.load(path, password)?;
        let page = doc
            .pages()
            .get(req.page.0 as i32)
            .map_err(|e| SbwbError::NotFound(format!("{}: {e}", req.page)))?;
        let (pw, ph) = (page.width().value as f64, page.height().value as f64);
        let crop = req.crop.unwrap_or(Rect::new(0.0, 0.0, pw, ph));
        let target_w = (crop.w * req.scale).round().max(1.0) as u32;
        let target_h = (crop.h * req.scale).round().max(1.0) as u32;
        if target_w > req.max_dimension || target_h > req.max_dimension {
            return Err(SbwbError::ResourceLimit(format!(
                "render of {target_w}x{target_h} px exceeds the {} px limit",
                req.max_dimension
            )));
        }
        // Render the whole page at the requested scale, then crop. PDFium's
        // clipping API is available but a plain crop keeps geometry exact.
        let full_w = (pw * req.scale).round().max(1.0) as u32;
        let full_h = (ph * req.scale).round().max(1.0) as u32;
        if req.crop.is_some() && (full_w > req.max_dimension * 4 || full_h > req.max_dimension * 4)
        {
            return Err(SbwbError::ResourceLimit(format!(
                "page render of {full_w}x{full_h} px exceeds the crop limit"
            )));
        }
        let cfg = PdfRenderConfig::new()
            .set_target_width(full_w as i32)
            .set_target_height(full_h as i32)
            .render_form_data(false)
            .use_grayscale_rendering(false);
        let bitmap = page
            .render_with_config(&cfg)
            .map_err(|e| SbwbError::other(format!("render failed: {e}")))?;
        let dynamic = bitmap
            .as_image()
            .map_err(|e| SbwbError::other(format!("bitmap conversion failed: {e}")))?;
        let rgb = dynamic.to_rgb8();
        let image = if req.crop.is_some() {
            let x0 = (crop.x * req.scale).round().max(0.0) as u32;
            let y0 = (crop.y * req.scale).round().max(0.0) as u32;
            let x0 = x0.min(rgb.width().saturating_sub(1));
            let y0 = y0.min(rgb.height().saturating_sub(1));
            let w = target_w.min(rgb.width() - x0);
            let h = target_h.min(rgb.height() - y0);
            image::imageops::crop_imm(&rgb, x0, y0, w, h).to_image()
        } else {
            rgb
        };
        let origin = if req.crop.is_some() {
            sbwb_core::Point {
                x: crop.x,
                y: crop.y,
            }
        } else {
            sbwb_core::Point::default()
        };
        Ok(Rendered {
            image,
            transform: Transform::uniform(req.scale).with_origin(origin),
            page_size: (pw, ph),
        })
    }

    /// Size of every page in points.
    pub fn page_sizes(&self, path: &Path, password: Option<&str>) -> Result<Vec<(f64, f64)>> {
        let doc = self.load(path, password)?;
        Ok(doc
            .pages()
            .iter()
            .map(|p| (p.width().value as f64, p.height().value as f64))
            .collect())
    }

    fn load(&self, path: &Path, password: Option<&str>) -> Result<PdfDocument<'_>> {
        self.pdfium
            .load_pdf_from_file(path, password)
            .map_err(|e| SbwbError::invalid(format!("cannot open {}: {e}", path.display())))
    }
}

/// Convenience for tests and tools.
pub fn render_page_to_png(
    pdfium_dir: &Path,
    pdf: &Path,
    page: PageIndex,
    dpi: u32,
    out: &Path,
) -> Result<PathBuf> {
    let r = Renderer::new(pdfium_dir)?;
    let rendered = r.render(pdf, None, &RenderRequest::page_at_dpi(page, dpi))?;
    rendered.image.save(out).map_err(SbwbError::other)?;
    Ok(out.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
    }

    #[test]
    fn renders_fixture_page_at_72_and_300_dpi() {
        let dir = crate::default_pdfium_dir();
        if !dir
            .join(if cfg!(windows) {
                "pdfium.dll"
            } else {
                "libpdfium.so"
            })
            .exists()
        {
            eprintln!("skipping: PDFium not present at {}", dir.display());
            return;
        }
        let r = Renderer::new(&dir).unwrap();
        assert_eq!(r.page_count(&fixture(), None).unwrap(), 110);
        let low = r
            .render(
                &fixture(),
                None,
                &RenderRequest::page_at_dpi(PageIndex(20), 72),
            )
            .unwrap();
        assert!(low.image.width() > 300 && low.image.width() < 400);
        let hi = r
            .render(
                &fixture(),
                None,
                &RenderRequest::page_at_dpi(PageIndex(20), 300),
            )
            .unwrap();
        assert!((hi.image.width() as f64 - low.image.width() as f64 * 300.0 / 72.0).abs() < 3.0);
        // the scan is a bilevel page: expect plenty of dark pixels (ink)
        let dark = hi.image.pixels().filter(|p| p[0] < 100).count();
        assert!(dark > 10_000, "dark pixels {dark}");
        // crop roundtrip
        let crop = Rect::new(50.0, 100.0, 100.0, 40.0);
        let c = r
            .render(
                &fixture(),
                None,
                &RenderRequest {
                    page: PageIndex(20),
                    scale: 300.0 / 72.0,
                    crop: Some(crop),
                    max_dimension: 4096,
                },
            )
            .unwrap();
        assert!((c.image.width() as f64 - 100.0 * 300.0 / 72.0).abs() < 2.0);
        let back = c.transform.to_points(&Rect::new(
            0.0,
            0.0,
            c.image.width() as f64,
            c.image.height() as f64,
        ));
        assert!((back.x - 50.0).abs() < 0.5 && (back.y - 100.0).abs() < 0.5);
    }

    #[test]
    fn oversized_render_is_refused() {
        let dir = crate::default_pdfium_dir();
        if !dir
            .join(if cfg!(windows) {
                "pdfium.dll"
            } else {
                "libpdfium.so"
            })
            .exists()
        {
            return;
        }
        let r = Renderer::new(&dir).unwrap();
        let req = RenderRequest {
            page: PageIndex(0),
            scale: 100.0,
            crop: None,
            max_dimension: 8192,
        };
        assert!(matches!(
            r.render(&fixture(), None, &req),
            Err(SbwbError::ResourceLimit(_))
        ));
    }

    /// NFR-03 / M2.6: uncached preview renders must stay well under the 2 s
    /// budget on the reference fixture; the numbers are printed for the
    /// benchmark record.
    #[test]
    fn render_timing_report() {
        let dir = crate::default_pdfium_dir();
        if !dir
            .join(if cfg!(windows) {
                "pdfium.dll"
            } else {
                "libpdfium.so"
            })
            .exists()
        {
            return;
        }
        let r = Renderer::new(&dir).unwrap();
        for (label, dpi) in [
            ("thumb 14dpi", 14u32),
            ("preview 72dpi", 72),
            ("preview 144dpi", 144),
            ("ocr 300dpi", 300),
        ] {
            let started = std::time::Instant::now();
            let out = r
                .render(
                    &fixture(),
                    None,
                    &RenderRequest::page_at_dpi(PageIndex(47), dpi),
                )
                .unwrap();
            let ms = started.elapsed().as_millis();
            eprintln!(
                "render {label}: {}x{} in {ms} ms",
                out.image.width(),
                out.image.height()
            );
            assert!(ms < 2000, "{label} took {ms} ms");
        }
    }
}
