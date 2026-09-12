//! Domain types shared by every SBWB layer (NFR-13).
//!
//! Nothing here performs I/O. Geometry is expressed in PDF points with the
//! origin at the top-left of the page, so preview, OCR, and export all share
//! one coordinate system (D-14, PROV-01).

pub mod error;
pub mod geometry;
pub mod ids;
pub mod page;
pub mod progress;

pub use error::{Result, SbwbError};
pub use geometry::{Point, Rect, Transform};
pub use ids::*;
pub use page::{PageIndex, PageScope, PageStatus};
pub use progress::{Progress, Stage, StageState};

/// Nominal DPI used for recognition renders (IMG-01).
pub const RECOGNITION_DPI: u32 = 300;
/// Points per inch in PDF user space.
pub const POINTS_PER_INCH: f64 = 72.0;
