//! Job execution inside the worker process.

use std::path::PathBuf;
use std::time::Instant;

use sbwb_core::{Result, SbwbError};

use crate::protocol::{Message, Request, RequestKind, Response};

#[derive(Default)]
pub struct Context {
    pdfium_dir: Option<PathBuf>,
    tessdata_dir: Option<PathBuf>,
    renderer: Option<sbwb_pdf::Renderer>,
}

impl Context {
    fn renderer(&mut self) -> Result<&sbwb_pdf::Renderer> {
        if self.renderer.is_none() {
            let dir = self
                .pdfium_dir
                .clone()
                .unwrap_or_else(sbwb_pdf::default_pdfium_dir);
            self.renderer = Some(sbwb_pdf::Renderer::new(&dir)?);
        }
        Ok(self.renderer.as_ref().unwrap())
    }
    fn tessdata(&self) -> PathBuf {
        self.tessdata_dir
            .clone()
            .unwrap_or_else(sbwb_ocr::default_tessdata_root)
    }
}

pub fn handle(ctx: &mut Context, req: Request, emit: &mut dyn FnMut(Message)) -> Result<Response> {
    let id = req.id;
    match req.kind {
        RequestKind::Ping => Ok(Response::Pong {
            pid: std::process::id(),
        }),
        RequestKind::Shutdown => Ok(Response::Ok),
        RequestKind::Configure {
            pdfium_dir,
            tessdata_dir,
        } => {
            ctx.pdfium_dir = Some(pdfium_dir);
            ctx.tessdata_dir = Some(tessdata_dir);
            ctx.renderer = None;
            Ok(Response::Ok)
        }
        RequestKind::InspectPdf {
            path,
            sample_pages,
            password,
        } => {
            emit(Message::Progress {
                id,
                activity: "inspecting".into(),
            });
            Ok(Response::PdfInfo(sbwb_pdf::inspect_file(
                &path,
                sample_pages,
                password.as_deref(),
            )?))
        }
        RequestKind::PageSizes { path, password } => {
            let r = ctx.renderer()?;
            Ok(Response::PageSizes {
                sizes: r.page_sizes(&path, password.as_deref())?,
            })
        }
        RequestKind::RenderPage {
            path,
            password,
            page,
            scale,
            crop,
            max_dimension,
            out,
        } => {
            let r = ctx.renderer()?;
            let req = sbwb_pdf::RenderRequest {
                page,
                scale,
                crop,
                max_dimension,
            };
            let rendered = r.render(&path, password.as_deref(), &req)?;
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = out.with_extension("part");
            save_image(&rendered.image, &tmp, &out)?;
            std::fs::rename(&tmp, &out)?;
            Ok(Response::Rendered {
                out,
                width: rendered.image.width(),
                height: rendered.image.height(),
                page_w_pt: rendered.page_size.0,
                page_h_pt: rendered.page_size.1,
                scale,
            })
        }
        RequestKind::OcrPage {
            path,
            password,
            page,
            dpi,
            settings,
            prep,
            save_render,
        } => {
            let started = Instant::now();
            emit(Message::Progress {
                id,
                activity: format!("render {page} @ {dpi}dpi"),
            });
            let rendered = {
                let r = ctx.renderer()?;
                r.render(
                    &path,
                    password.as_deref(),
                    &sbwb_pdf::RenderRequest::page_at_dpi(page, dpi),
                )?
            };
            if let Some(p) = &save_render {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let tmp = p.with_extension("part");
                save_image(&rendered.image, &tmp, p)?;
                std::fs::rename(&tmp, p)?;
            }
            emit(Message::Progress {
                id,
                activity: format!("prepare {page}"),
            });
            let (prepared, report) = sbwb_image::prepare(&rendered.image, &prep);
            emit(Message::Progress {
                id,
                activity: format!("ocr {page}"),
            });
            let engine = sbwb_ocr::Engine::new(&ctx.tessdata());
            let mut output = engine.recognize(&prepared, &rendered.transform, &settings)?;
            if report.deskewed {
                // Boxes came from the deskewed bitmap; anchor them to the
                // original render, then to page points (PROV-01).
                for w in &mut output.words {
                    w.bbox_px = report.map_back(&w.bbox_px);
                    w.bbox = rendered.transform.to_points(&w.bbox_px);
                }
                for l in &mut output.lines {
                    l.bbox_px = report.map_back(&l.bbox_px);
                    l.bbox = rendered.transform.to_points(&l.bbox_px);
                }
                for b in &mut output.blocks {
                    b.bbox_px = report.map_back(&b.bbox_px);
                    b.bbox = rendered.transform.to_points(&b.bbox_px);
                }
            }
            Ok(Response::Ocr {
                output,
                page_w_pt: rendered.page_size.0,
                page_h_pt: rendered.page_size.1,
                render: save_render,
                prep: report,
                elapsed_ms: started.elapsed().as_millis() as u64,
            })
        }
        RequestKind::LayoutPage {
            path,
            password,
            page,
            words,
            lines,
            blocks,
            settings,
        } => {
            let started = Instant::now();
            emit(Message::Progress {
                id,
                activity: format!("layout {page}"),
            });
            let rendered = {
                let r = ctx.renderer()?;
                r.render(
                    &path,
                    password.as_deref(),
                    &sbwb_pdf::RenderRequest::page_at_dpi(page, sbwb_layout::ANALYSIS_DPI),
                )?
            };
            let gray = image::imageops::grayscale(&rendered.image);
            let input = sbwb_layout::LayoutInput {
                page_w: rendered.page_size.0,
                page_h: rendered.page_size.1,
                words: &words,
                lines: &lines,
                blocks: &blocks,
            };
            let out = sbwb_layout::analyze(&gray, &rendered.transform, &input, &settings);
            Ok(Response::Layout {
                regions: out.regions,
                report: out.report,
                algorithm: out.algorithm,
                elapsed_ms: started.elapsed().as_millis() as u64,
            })
        }
        #[cfg(any(test, debug_assertions))]
        RequestKind::Sleep { ms } => {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Ok(Response::Ok)
        }
    }
}

fn save_image(
    img: &image::RgbImage,
    tmp: &std::path::Path,
    final_name: &std::path::Path,
) -> Result<()> {
    let ext = final_name
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let fmt = match ext.as_str() {
        "webp" => image::ImageFormat::WebP,
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        _ => image::ImageFormat::Png,
    };
    img.save_with_format(tmp, fmt)
        .map_err(|e| SbwbError::other(format!("save {}: {e}", final_name.display())))
}
