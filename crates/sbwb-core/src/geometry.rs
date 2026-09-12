use serde::{Deserialize, Serialize};

/// A point in PDF page space (points, origin top-left, y down).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle in PDF page space (points, origin top-left).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn from_corners(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        let (x, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        Self {
            x,
            y,
            w: x1 - x,
            h: y1 - y,
        }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn area(&self) -> f64 {
        self.w * self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
    pub fn center(&self) -> Point {
        Point {
            x: self.x + self.w / 2.0,
            y: self.y + self.h / 2.0,
        }
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn intersection(&self, o: &Rect) -> Option<Rect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        (x1 > x0 && y1 > y0).then(|| Rect::from_corners(x0, y0, x1, y1))
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Rect::from_corners(
            self.x.min(o.x),
            self.y.min(o.y),
            self.right().max(o.right()),
            self.bottom().max(o.bottom()),
        )
    }
    /// Fraction of `self` covered by `o`, in 0..=1.
    pub fn overlap_fraction(&self, o: &Rect) -> f64 {
        if self.area() <= 0.0 {
            return 0.0;
        }
        self.intersection(o)
            .map(|i| i.area() / self.area())
            .unwrap_or(0.0)
    }
    pub fn inflate(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(
            self.x - dx,
            self.y - dy,
            self.w + 2.0 * dx,
            self.h + 2.0 * dy,
        )
    }
}

/// Affine transform between PDF page space and a bitmap (scale + offset).
///
/// `pixel = (point - origin) * scale`; used both for preview highlights and
/// for mapping OCR word boxes back to page space (PROV-01).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Pixels per point along x.
    pub sx: f64,
    /// Pixels per point along y.
    pub sy: f64,
    /// Page-space origin of the bitmap (for crops).
    pub origin: Point,
}

impl Transform {
    pub fn uniform(scale: f64) -> Self {
        Self {
            sx: scale,
            sy: scale,
            origin: Point::default(),
        }
    }
    pub fn for_dpi(dpi: u32) -> Self {
        Self::uniform(dpi as f64 / crate::POINTS_PER_INCH)
    }
    pub fn with_origin(mut self, origin: Point) -> Self {
        self.origin = origin;
        self
    }
    pub fn to_pixels(&self, r: &Rect) -> Rect {
        Rect::new(
            (r.x - self.origin.x) * self.sx,
            (r.y - self.origin.y) * self.sy,
            r.w * self.sx,
            r.h * self.sy,
        )
    }
    pub fn to_points(&self, px: &Rect) -> Rect {
        Rect::new(
            px.x / self.sx + self.origin.x,
            px.y / self.sy + self.origin.y,
            px.w / self.sx,
            px.h / self.sy,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_roundtrip() {
        let t = Transform::for_dpi(300).with_origin(Point { x: 10.0, y: 20.0 });
        let r = Rect::new(30.0, 40.0, 50.0, 60.0);
        let px = t.to_pixels(&r);
        assert!((px.x - 20.0 * 300.0 / 72.0).abs() < 1e-9);
        let back = t.to_points(&px);
        assert!((back.x - r.x).abs() < 1e-9 && (back.h - r.h).abs() < 1e-9);
    }

    #[test]
    fn overlap_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!((a.overlap_fraction(&b) - 0.25).abs() < 1e-9);
        let u = a.union(&b);
        assert_eq!(u, Rect::new(0.0, 0.0, 15.0, 15.0));
        assert!(a.intersection(&Rect::new(20.0, 20.0, 1.0, 1.0)).is_none());
    }
}
