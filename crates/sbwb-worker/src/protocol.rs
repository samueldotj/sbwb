use std::path::PathBuf;

use sbwb_core::{PageIndex, Rect};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub kind: RequestKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestKind {
    Ping,
    Shutdown,
    /// Configure resource directories once per worker.
    Configure {
        pdfium_dir: PathBuf,
        tessdata_dir: PathBuf,
    },
    InspectPdf {
        path: PathBuf,
        sample_pages: u32,
    },
    /// Render a page (or crop) and write it to `out` as PNG or WebP.
    RenderPage {
        path: PathBuf,
        password: Option<String>,
        page: PageIndex,
        scale: f64,
        crop: Option<Rect>,
        max_dimension: u32,
        out: PathBuf,
    },
    /// Render at `dpi` and OCR the whole page; returns words in page points.
    OcrPage {
        path: PathBuf,
        password: Option<String>,
        page: PageIndex,
        dpi: u32,
        settings: sbwb_ocr::OcrSettings,
        /// Optional path to save the recognition render (evidence).
        save_render: Option<PathBuf>,
    },
    /// Sleep for testing timeouts.
    #[cfg(any(test, debug_assertions))]
    Sleep {
        ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Pong {
        pid: u32,
    },
    PdfInfo(sbwb_pdf::PdfInfo),
    Rendered {
        out: PathBuf,
        width: u32,
        height: u32,
        page_w_pt: f64,
        page_h_pt: f64,
        scale: f64,
    },
    Ocr {
        output: sbwb_ocr::OcrOutput,
        page_w_pt: f64,
        page_h_pt: f64,
        render: Option<PathBuf>,
        elapsed_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum Message {
    Heartbeat { id: u64 },
    Progress { id: u64, activity: String },
    Result { id: u64, result: Response },
    Error { id: u64, error: String },
}

impl Message {
    pub fn id(&self) -> u64 {
        match self {
            Message::Heartbeat { id }
            | Message::Progress { id, .. }
            | Message::Result { id, .. }
            | Message::Error { id, .. } => *id,
        }
    }
}
