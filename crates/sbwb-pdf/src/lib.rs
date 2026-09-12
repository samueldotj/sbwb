//! PDF access for SBWB.
//!
//! * [`inspect`]: structural facts without rendering (page count, media
//!   boxes, encryption, metadata) via `lopdf`, plus the source checksum.
//! * [`render`]: page rasterization via PDFium at any scale (IMG-01, D-14).
//!
//! PDFium is loaded dynamically from a directory chosen by the caller so the
//! same code serves the packaged app and the development checkout.

pub mod inspect;
pub mod render;

pub use inspect::{inspect_file, PdfInfo, PdfPageInfo};
pub use render::{RenderRequest, Rendered, Renderer};

/// Directory containing the platform PDFium shared library.
pub fn default_pdfium_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("SBWB_PDFIUM_DIR") {
        return d.into();
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/bin")
}
