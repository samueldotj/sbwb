//! Conservative page preparation (IMG-01, M3.4).
//!
//! * Deskew: the skew angle is estimated on a downsampled binarized copy by
//!   maximizing the variance of the horizontal projection profile (text
//!   lines produce sharp peaks when level), then the full bitmap is rotated
//!   about its centre with a white fill. Angles below 0.15° are left alone.
//! * Despeckle: optional 3×3 median filter for salt-and-pepper noise.
//!
//! Every transform is recorded in [`PrepReport`], and [`PrepReport::map_back`]
//! maps rectangles in the prepared bitmap back to the original render so OCR
//! word boxes stay anchored to the source (PROV-01).

use image::{GrayImage, RgbImage};
use imageproc::geometric_transformations::{rotate_about_center, Border, Interpolation};
use sbwb_core::Rect;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepSettings {
    pub deskew: bool,
    /// Maximum absolute skew angle (degrees) that will be corrected.
    pub max_skew_deg: f64,
    pub despeckle: bool,
}

impl Default for PrepSettings {
    fn default() -> Self {
        Self {
            deskew: true,
            max_skew_deg: 5.0,
            despeckle: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrepReport {
    pub input_px: (u32, u32),
    pub output_px: (u32, u32),
    /// Estimated skew in degrees (positive = content rotated clockwise).
    pub skew_deg: f64,
    /// Rotation applied to the bitmap, in degrees, clockwise positive.
    pub applied_deg: f64,
    pub deskewed: bool,
    pub despeckled: bool,
    pub warnings: Vec<String>,
}

impl PrepReport {
    /// Map a rectangle in prepared-bitmap pixels back to original-bitmap
    /// pixels (bounding box of the un-rotated corners).
    pub fn map_back(&self, r: &Rect) -> Rect {
        if self.applied_deg.abs() < 1e-9 {
            return *r;
        }
        let (cx, cy) = (self.input_px.0 as f64 / 2.0, self.input_px.1 as f64 / 2.0);
        // The prepared image was produced by rotating the original clockwise
        // by `applied_deg`; undo with a counter-clockwise rotation.
        let theta = -self.applied_deg.to_radians();
        let (s, c) = theta.sin_cos();
        let corners = [
            (r.x, r.y),
            (r.right(), r.y),
            (r.x, r.bottom()),
            (r.right(), r.bottom()),
        ];
        let mut minx = f64::MAX;
        let mut miny = f64::MAX;
        let mut maxx = f64::MIN;
        let mut maxy = f64::MIN;
        for (x, y) in corners {
            let (dx, dy) = (x - cx, y - cy);
            let nx = cx + dx * c - dy * s;
            let ny = cy + dx * s + dy * c;
            minx = minx.min(nx);
            miny = miny.min(ny);
            maxx = maxx.max(nx);
            maxy = maxy.max(ny);
        }
        Rect::from_corners(minx, miny, maxx, maxy)
    }
}

fn to_gray(img: &RgbImage) -> GrayImage {
    image::imageops::grayscale(img)
}

/// Score for a candidate rotation: variance of the row ink profile.
fn projection_score(small: &GrayImage, angle_deg: f64) -> f64 {
    let rotated = rotate_about_center(
        small,
        (angle_deg as f32).to_radians(),
        Interpolation::Nearest,
        Border::Constant(image::Luma([255u8])),
    );
    let (w, h) = rotated.dimensions();
    let mut rows = vec![0u32; h as usize];
    for (y, row) in rows.iter_mut().enumerate() {
        let mut n = 0u32;
        for x in 0..w {
            if rotated.get_pixel(x, y as u32)[0] < 128 {
                n += 1;
            }
        }
        *row = n;
    }
    let mean = rows.iter().map(|&v| v as f64).sum::<f64>() / rows.len().max(1) as f64;
    rows.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>()
}

/// Estimate the skew angle in degrees within ±max_deg (coarse then fine).
pub fn estimate_skew(gray: &GrayImage, max_deg: f64) -> f64 {
    let (w, h) = gray.dimensions();
    let target_w = 500u32;
    let small = if w > target_w {
        let nh = (h as f64 * target_w as f64 / w as f64).round().max(1.0) as u32;
        image::imageops::resize(gray, target_w, nh, image::imageops::FilterType::Triangle)
    } else {
        gray.clone()
    };
    let mut best = (0.0f64, f64::MIN);
    let mut a = -max_deg;
    while a <= max_deg + 1e-9 {
        let s = projection_score(&small, a);
        if s > best.1 {
            best = (a, s);
        }
        a += 0.5;
    }
    let coarse = best.0;
    let mut a = coarse - 0.5;
    while a <= coarse + 0.5 + 1e-9 {
        let s = projection_score(&small, a);
        if s > best.1 {
            best = (a, s);
        }
        a += 0.1;
    }
    // The score is maximal when rotating by -skew levels the lines, so the
    // best rotation angle is the negative of the content skew.
    -best.0
}

/// Prepare a page bitmap for recognition.
pub fn prepare(img: &RgbImage, settings: &PrepSettings) -> (RgbImage, PrepReport) {
    let mut report = PrepReport {
        input_px: (img.width(), img.height()),
        output_px: (img.width(), img.height()),
        ..Default::default()
    };
    let mut out = img.clone();
    if settings.deskew {
        let gray = to_gray(img);
        let skew = estimate_skew(&gray, settings.max_skew_deg);
        report.skew_deg = skew;
        if skew.abs() >= 0.15 && skew.abs() <= settings.max_skew_deg {
            // Content is rotated clockwise by `skew`; rotate counter-clockwise
            // (negative clockwise angle) to level it.
            let applied = -skew;
            out = rotate_about_center(
                img,
                (applied as f32).to_radians(),
                Interpolation::Bilinear,
                Border::Constant(image::Rgb([255u8, 255, 255])),
            );
            report.applied_deg = applied;
            report.deskewed = true;
        } else if skew.abs() > settings.max_skew_deg {
            report.warnings.push(format!(
                "skew of {skew:.1}° exceeds the {:.1}° correction limit; left as is",
                settings.max_skew_deg
            ));
        }
    }
    if settings.despeckle {
        out = imageproc::filter::median_filter(&out, 1, 1);
        report.despeckled = true;
    }
    report.output_px = (out.width(), out.height());
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic page: horizontal black "text lines", rotated by a known angle.
    fn synthetic(skew_deg: f64) -> RgbImage {
        let (w, h) = (800u32, 1000u32);
        let mut img = RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
        for line in 0..30 {
            let y0 = 120 + line * 26;
            for y in y0..y0 + 8 {
                for x in 100..700 {
                    img.put_pixel(x, y, image::Rgb([0, 0, 0]));
                }
            }
        }
        rotate_about_center(
            &img,
            (skew_deg as f32).to_radians(),
            Interpolation::Bilinear,
            Border::Constant(image::Rgb([255, 255, 255])),
        )
    }

    #[test]
    fn estimates_and_corrects_skew() {
        let skewed = synthetic(2.0);
        let (fixed, report) = prepare(&skewed, &PrepSettings::default());
        assert!(
            (report.skew_deg - 2.0).abs() < 0.3,
            "estimated {}",
            report.skew_deg
        );
        assert!(report.deskewed);
        // After correction the projection profile should be sharper than before.
        let before = projection_score(&to_gray(&skewed), 0.0);
        let after = projection_score(&to_gray(&fixed), 0.0);
        assert!(after > before * 1.5, "before {before} after {after}");
    }

    #[test]
    fn leaves_level_pages_alone() {
        let level = synthetic(0.0);
        let (_, report) = prepare(&level, &PrepSettings::default());
        assert!(!report.deskewed);
        assert!(report.skew_deg.abs() < 0.15);
    }

    #[test]
    fn map_back_returns_to_original_pixels() {
        // Place a small black square, rotate it as prepare() would, find it,
        // and check that map_back lands on the original location.
        let (w, h) = (600u32, 800u32);
        let mut img = RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
        for y in 200..220 {
            for x in 400..420 {
                img.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        let applied = 3.0f64;
        let rotated = rotate_about_center(
            &img,
            (applied as f32).to_radians(),
            Interpolation::Nearest,
            Border::Constant(image::Rgb([255, 255, 255])),
        );
        // centroid of dark pixels in the rotated image
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for (x, y, p) in rotated.enumerate_pixels() {
            if p[0] < 128 {
                sx += x as f64;
                sy += y as f64;
                n += 1.0;
            }
        }
        let (cx, cy) = (sx / n, sy / n);
        let report = PrepReport {
            input_px: (w, h),
            output_px: (w, h),
            applied_deg: applied,
            deskewed: true,
            ..Default::default()
        };
        let back = report
            .map_back(&Rect::new(cx - 1.0, cy - 1.0, 2.0, 2.0))
            .center();
        assert!(
            (back.x - 410.0).abs() < 2.0 && (back.y - 210.0).abs() < 2.0,
            "mapped to {back:?}"
        );
    }
}
