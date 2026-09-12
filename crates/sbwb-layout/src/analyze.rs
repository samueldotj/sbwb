//! Classical page analysis (D-11): components → line blobs → blocks →
//! classified regions → reading order → coverage.

use image::GrayImage;
use imageproc::region_labelling::{connected_components, Connectivity};
use sbwb_core::{Rect, Transform};
use sbwb_ocr::{OcrBlock, OcrLine, OcrWord};
use serde::{Deserialize, Serialize};

use crate::region::{Region, RegionKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSettings {
    /// Gray level below which a pixel counts as ink.
    pub ink_threshold: u8,
    /// Horizontal gap (in text heights) that still joins components into a line.
    pub line_gap_factor: f64,
    /// Vertical gap (in text heights) that still joins lines into a block.
    pub block_gap_factor: f64,
    /// Vertical gap (in text heights) below which adjacent body blocks merge.
    pub body_merge_factor: f64,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            ink_threshold: 140,
            line_gap_factor: 1.6,
            block_gap_factor: 1.3,
            body_merge_factor: 3.0,
        }
    }
}

pub struct LayoutInput<'a> {
    pub page_w: f64,
    pub page_h: f64,
    pub words: &'a [OcrWord],
    pub lines: &'a [OcrLine],
    pub blocks: &'a [OcrBlock],
}

/// Coverage accountability for one page (LAY-03).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CoverageReport {
    pub words_total: u32,
    /// OCR words not inside any text region.
    pub words_uncovered: u32,
    /// Ink line blobs with no OCR word on them (probable missing text),
    /// in page points.
    pub uncovered_text: Vec<Rect>,
    /// Regions whose ink touches the page edge (possible clipping).
    pub clipped_regions: u32,
    /// Tesseract blocks that span more than one of our regions.
    pub segmentation_disagreements: u32,
    /// Detected column gutters (x ranges in points) inside the body band.
    pub gutters: Vec<(f64, f64)>,
    pub columns: u8,
    /// Median text height in points (an x-height proxy).
    pub text_height_pt: f64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutOutput {
    pub regions: Vec<Region>,
    pub report: CoverageReport,
    pub algorithm: String,
}

#[derive(Debug, Clone)]
struct Blob {
    rect: Rect, // pixels
    count: u32,
    mean_h: f64,
}

fn union(a: &Rect, b: &Rect) -> Rect {
    a.union(b)
}

fn v_overlap(a: &Rect, b: &Rect) -> f64 {
    let top = a.y.max(b.y);
    let bot = a.bottom().min(b.bottom());
    (bot - top).max(0.0)
}

fn h_overlap(a: &Rect, b: &Rect) -> f64 {
    let l = a.x.max(b.x);
    let r = a.right().min(b.right());
    (r - l).max(0.0)
}

fn h_gap(a: &Rect, b: &Rect) -> f64 {
    if a.right() < b.x {
        b.x - a.right()
    } else if b.right() < a.x {
        a.x - b.right()
    } else {
        0.0
    }
}

fn v_gap(a: &Rect, b: &Rect) -> f64 {
    if a.bottom() < b.y {
        b.y - a.bottom()
    } else if b.bottom() < a.y {
        a.y - b.bottom()
    } else {
        0.0
    }
}

/// Connected-component bounding boxes in pixels, with noise and page
/// borders removed.
fn components(gray: &GrayImage, ink: u8) -> Vec<Rect> {
    let (w, h) = gray.dimensions();
    let mut bin = GrayImage::new(w, h);
    for (x, y, p) in gray.enumerate_pixels() {
        bin.put_pixel(x, y, image::Luma([if p[0] < ink { 255 } else { 0 }]));
    }
    let labels = connected_components(&bin, Connectivity::Eight, image::Luma([0u8]));
    let mut boxes: Vec<(u32, u32, u32, u32, u32)> = Vec::new(); // minx,miny,maxx,maxy,count
    let mut index: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (x, y, p) in labels.enumerate_pixels() {
        let l = p[0];
        if l == 0 {
            continue;
        }
        match index.get(&l) {
            Some(&i) => {
                let b = &mut boxes[i];
                b.0 = b.0.min(x);
                b.1 = b.1.min(y);
                b.2 = b.2.max(x);
                b.3 = b.3.max(y);
                b.4 += 1;
            }
            None => {
                index.insert(l, boxes.len());
                boxes.push((x, y, x, y, 1));
            }
        }
    }
    let (pw, ph) = (w as f64, h as f64);
    boxes
        .into_iter()
        .filter_map(|(x0, y0, x1, y1, n)| {
            let r = Rect::new(
                x0 as f64,
                y0 as f64,
                (x1 - x0 + 1) as f64,
                (y1 - y0 + 1) as f64,
            );
            if n < 3 || r.w < 2.0 || r.h < 2.0 {
                return None; // speckle
            }
            if r.w > 0.9 * pw || r.h > 0.5 * ph {
                return None; // border or scan frame
            }
            // long thin horizontal rules are separators, kept for footnotes
            Some(r)
        })
        .collect()
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Union-find grouping of rectangles by a pairwise predicate (O(n²) on a
/// few thousand components is fine at 100 dpi).
fn group_by<F: Fn(&Rect, &Rect) -> bool>(items: &[Rect], joins: F) -> Vec<Vec<usize>> {
    let n = items.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    // sort by x for a cheap pruning window
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| items[a].x.partial_cmp(&items[b].x).unwrap());
    for (k, &i) in order.iter().enumerate() {
        for &j in order.iter().skip(k + 1) {
            if items[j].x > items[i].right() + 200.0 {
                break;
            }
            if joins(&items[i], &items[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().collect()
}

fn blobs_from_groups(items: &[Rect], groups: Vec<Vec<usize>>) -> Vec<Blob> {
    groups
        .into_iter()
        .map(|g| {
            let mut rect = items[g[0]];
            let mut hs = 0.0;
            for &i in &g {
                rect = union(&rect, &items[i]);
                hs += items[i].h;
            }
            Blob {
                rect,
                count: g.len() as u32,
                mean_h: hs / g.len() as f64,
            }
        })
        .collect()
}

/// Vertical whitespace gutters inside `band` (pixels): x ranges at least
/// `min_w` wide whose ink coverage is below `max_cover` of the band height.
/// Computed from raw components so a continuous gutter between a side-note
/// column and the body is found even when word gaps are as wide.
fn gutters(items: &[Rect], band: &Rect, min_w: f64, max_cover: f64) -> Vec<(f64, f64)> {
    let w = band.w.max(1.0) as usize;
    let mut cover = vec![0.0f64; w];
    for r in items {
        if v_overlap(r, band) <= 0.0 {
            continue;
        }
        let x0 = (r.x - band.x).max(0.0) as usize;
        let x1 = ((r.right() - band.x).min(band.w)) as usize;
        for c in cover.iter_mut().take(x1.min(w)).skip(x0) {
            *c += r.h;
        }
    }
    let limit = max_cover * band.h;
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for x in 0..=w {
        let empty = x < w && cover[x] <= limit;
        match (empty, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                if (x - s) as f64 >= min_w && s > 0 && x < w {
                    out.push((band.x + s as f64, band.x + x as f64));
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// True when `a` and `b` lie on opposite sides of a gutter's midline.
/// Centres are compared rather than edges so a glyph that pokes a little
/// way into the gutter cannot bridge it.
fn crosses_gutter(a: &Rect, b: &Rect, gutters: &[(f64, f64)]) -> bool {
    let (ca, cb) = (a.center().x, b.center().x);
    gutters.iter().any(|(g0, g1)| {
        let mid = (g0 + g1) / 2.0;
        (ca < mid) != (cb < mid)
    })
}

pub fn analyze(
    gray: &GrayImage,
    transform: &Transform,
    input: &LayoutInput,
    settings: &AnalysisSettings,
) -> LayoutOutput {
    let (pw, ph) = (gray.width() as f64, gray.height() as f64);
    let mut report = CoverageReport::default();
    let comps = components(gray, settings.ink_threshold);
    if comps.is_empty() {
        report.warnings.push("no ink found on the page".into());
        return LayoutOutput {
            regions: vec![],
            report,
            algorithm: ALGORITHM.into(),
        };
    }
    // text height estimate: median height of medium components
    let mut hs: Vec<f64> = comps
        .iter()
        .filter(|r| r.h >= 4.0 && r.h <= 60.0 && r.w <= 80.0)
        .map(|r| r.h)
        .collect();
    let h = median(&mut hs).max(4.0);
    report.text_height_pt = transform.to_points(&Rect::new(0.0, 0.0, 0.0, h)).h;

    // separators: long thin rules
    let rules: Vec<Rect> = comps
        .iter()
        .filter(|r| r.w > 3.0 * h && r.h <= 3.0)
        .copied()
        .collect();
    let glyphs: Vec<Rect> = comps
        .iter()
        .filter(|r| !(r.w > 3.0 * h && r.h <= 3.0))
        .copied()
        .collect();

    // 1. gutters from raw glyphs inside the text band
    let band = glyphs
        .iter()
        .fold(None::<Rect>, |acc, r| {
            Some(acc.map(|a| a.union(r)).unwrap_or(*r))
        })
        .unwrap_or(Rect::new(0.0, 0.0, pw, ph));
    let gutter_px: Vec<(f64, f64)> = gutters(&glyphs, &band, 0.4 * h, 0.08)
        .into_iter()
        .filter(|(g0, g1)| {
            let left = glyphs.iter().filter(|r| r.right() <= *g0).count();
            let right = glyphs.iter().filter(|r| r.x >= *g1).count();
            left >= 8 && right >= 8
        })
        .collect();

    // 2. line blobs: never join across a gutter
    let line_gap = settings.line_gap_factor * h;
    let groups = group_by(&glyphs, |a, b| {
        let vo = v_overlap(a, b);
        let smaller = a.h.min(b.h);
        vo >= 0.5 * smaller && h_gap(a, b) <= line_gap && !crosses_gutter(a, b, &gutter_px)
    });
    let mut lines = blobs_from_groups(&glyphs, groups);
    lines.retain(|l| l.count >= 2 || l.rect.w >= h);

    // 3. blocks: vertically adjacent, horizontally overlapping lines
    let line_rects: Vec<Rect> = lines.iter().map(|l| l.rect).collect();
    let block_gap = settings.block_gap_factor * h;
    let groups = group_by(&line_rects, |a, b| {
        let ho = h_overlap(a, b);
        let narrower = a.w.min(b.w);
        ho >= 0.3 * narrower && v_gap(a, b) <= block_gap && !crosses_gutter(a, b, &gutter_px)
    });
    // Split each block at vertical gaps well above its own line pitch, and
    // peel off a running head at the top or a catchword/footer line at the
    // bottom, so those do not merge with the body; then build blobs.
    let mut blocks: Vec<Blob> = Vec::new();
    for g in groups {
        let mut idx = g.clone();
        idx.sort_by(|&a, &b| lines[a].rect.y.partial_cmp(&lines[b].rect.y).unwrap());
        // rows: lines whose vertical centres sit within half a text height
        let mut rows: Vec<Rect> = Vec::new();
        let mut row_of: Vec<usize> = Vec::with_capacity(idx.len());
        for &i in &idx {
            let r = lines[i].rect;
            match rows.last_mut() {
                Some(last) if (last.center().y - r.center().y).abs() < 0.5 * h => {
                    *last = last.union(&r);
                    row_of.push(rows.len() - 1);
                }
                _ => {
                    rows.push(r);
                    row_of.push(rows.len() - 1);
                }
            }
        }
        let gaps: Vec<f64> = rows
            .windows(2)
            .map(|w| (w[1].y - w[0].bottom()).max(0.0))
            .collect();
        let med = median(&mut gaps.clone());
        let block_w = rows.iter().fold(0.0f64, |m, r| m.max(r.w));
        let n = rows.len();
        let mut split_after = vec![false; n.saturating_sub(1)];
        for (k, gap) in gaps.iter().enumerate() {
            if *gap > (2.5 * med).max(1.4 * h) {
                split_after[k] = true;
            }
        }
        if n >= 4 && rows[0].y < 0.12 * ph {
            let rest = median(&mut gaps[1..].to_vec());
            if gaps[0] >= 1.5 * rest.max(1.0) || rows[0].w <= 0.8 * block_w {
                split_after[0] = true;
            }
        }
        if n >= 4 && rows[n - 1].bottom() > 0.9 * ph && rows[n - 1].w <= 0.6 * block_w {
            split_after[n - 2] = true;
        }
        if std::env::var_os("SBWB_LAYOUT_DEBUG").is_some() && n > 4 {
            let head: Vec<String> = rows
                .iter()
                .take(3)
                .map(|r| {
                    format!(
                        "y{:.0}..{:.0} x{:.0}..{:.0}",
                        r.y,
                        r.bottom(),
                        r.x,
                        r.right()
                    )
                })
                .collect();
            eprintln!(
                "  rows[{n}] first={} gaps={:?} med={med:.1} splits={:?}",
                head.join(" | "),
                &gaps[..gaps.len().min(3)],
                split_after
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| **s)
                    .map(|(k, _)| k)
                    .collect::<Vec<_>>()
            );
        }
        // segment id per row
        let mut seg_of_row = vec![0usize; n];
        for k in 1..n {
            seg_of_row[k] = seg_of_row[k - 1] + usize::from(split_after[k - 1]);
        }
        let seg_count = seg_of_row.last().copied().unwrap_or(0) + 1;
        let mut segments: Vec<Vec<usize>> = vec![vec![]; seg_count];
        for (pos, &i) in idx.iter().enumerate() {
            segments[seg_of_row[row_of[pos]]].push(i);
        }
        for seg in segments.into_iter().filter(|s| !s.is_empty()) {
            let mut rect = lines[seg[0]].rect;
            let mut hsum = 0.0;
            for &i in &seg {
                rect = union(&rect, &lines[i].rect);
                hsum += lines[i].mean_h;
            }
            blocks.push(Blob {
                rect,
                count: seg.len() as u32,
                mean_h: hsum / seg.len() as f64,
            });
        }
    }

    if std::env::var_os("SBWB_LAYOUT_DEBUG").is_some() {
        eprintln!("h={h:.1}px band={band:?} gutters={gutter_px:?}");
        for b in &blocks {
            eprintln!(
                "  block x={:.0}..{:.0} y={:.0}..{:.0} lines={} mean_h={:.1}",
                b.rect.x,
                b.rect.right(),
                b.rect.y,
                b.rect.bottom(),
                b.count,
                b.mean_h
            );
        }
    }

    // 4. main column(s): widest blocks with several lines
    let widest = blocks.iter().map(|b| b.rect.w).fold(0.0, f64::max);
    let main: Vec<&Blob> = blocks
        .iter()
        .filter(|b| b.rect.w >= 0.6 * widest && b.count >= 3)
        .collect();
    let main_x0 = main.iter().map(|b| b.rect.x).fold(f64::MAX, f64::min);
    let main_x1 = main.iter().map(|b| b.rect.right()).fold(f64::MIN, f64::max);
    let main_top = main.iter().map(|b| b.rect.y).fold(f64::MAX, f64::min);
    let main_bottom = main
        .iter()
        .map(|b| b.rect.bottom())
        .fold(f64::MIN, f64::max);
    let has_main = !main.is_empty();

    // 5. classify
    let mut classified: Vec<(RegionKind, Blob, f32)> = Vec::new();
    for b in blocks.drain(..) {
        let r = &b.rect;
        let small_text = b.mean_h < 0.88 * h;
        let inside_main_x = has_main && r.x >= main_x0 - 2.0 * h && r.right() <= main_x1 + 2.0 * h;
        let outside_main_x =
            has_main && (r.right() <= main_x0 - 0.5 * h || r.x >= main_x1 + 0.5 * h);
        let short = b.count <= 2;
        let kind = if r.y < 0.12 * ph && short && (!has_main || r.bottom() <= main_top + h) {
            if r.w < 3.0 * h {
                RegionKind::PageNumber
            } else {
                RegionKind::Header
            }
        } else if r.bottom() > 0.9 * ph && short && (!has_main || r.y >= main_bottom - h) {
            if r.w < 5.0 * h && r.right() >= main_x1 - 4.0 * h {
                RegionKind::Catchword
            } else {
                RegionKind::Footer
            }
        } else if outside_main_x && r.w < 0.35 * pw {
            RegionKind::Marginalia
        } else if has_main && inside_main_x && r.y >= main_bottom - h && small_text {
            RegionKind::Footnote
        } else if has_main
            && inside_main_x
            && rules
                .iter()
                .any(|s| s.y < r.y && s.y > main_top && (r.y - s.y) < 6.0 * h)
            && small_text
        {
            RegionKind::Footnote
        } else if inside_main_x
            && (b.mean_h > 1.25 * h || (r.w < 0.7 * widest && b.count <= 3 && r.y < main_top + h))
        {
            RegionKind::Heading
        } else if inside_main_x || (!has_main && b.count >= 1) {
            RegionKind::Body
        } else {
            RegionKind::Uncertain
        };
        let score = match kind {
            RegionKind::Body
            | RegionKind::Header
            | RegionKind::Marginalia
            | RegionKind::Footnote => 0.9,
            RegionKind::Uncertain => 0.3,
            _ => 0.7,
        };
        classified.push((kind, b, score));
    }

    // 6. merge body blocks that stack vertically in the same column (a side
    // note between them in y order must not break the chain)
    classified.sort_by(|a, b| a.1.rect.y.partial_cmp(&b.1.rect.y).unwrap());
    let merge_gap = settings.body_merge_factor * h;
    let mut merged: Vec<(RegionKind, Blob, f32)> = Vec::new();
    for item in classified {
        if item.0 == RegionKind::Body {
            let target = merged.iter_mut().rev().find(|m| {
                m.0 == RegionKind::Body
                    && v_gap(&m.1.rect, &item.1.rect) <= merge_gap
                    && h_overlap(&m.1.rect, &item.1.rect) >= 0.5 * m.1.rect.w.min(item.1.rect.w)
                    && !crosses_gutter(&m.1.rect, &item.1.rect, &gutter_px)
            });
            if let Some(m) = target {
                m.1.rect = m.1.rect.union(&item.1.rect);
                m.1.count += item.1.count;
                continue;
            }
        }
        merged.push(item);
    }

    // 7. column assignment: only gutters strictly inside the main text
    // span count as column gutters (side-note gutters lie outside it)
    let column_gutters: Vec<(f64, f64)> = gutter_px
        .iter()
        .copied()
        .filter(|(g0, g1)| has_main && *g0 > main_x0 + h && *g1 < main_x1 - h)
        .collect();
    let column_of = |r: &Rect| -> Option<u8> {
        if column_gutters.is_empty() {
            return None;
        }
        let mut c = 0u8;
        for (g0, _) in &column_gutters {
            if r.x >= *g0 {
                c += 1;
            }
        }
        Some(c)
    };

    // 8. regions in page points
    let mut regions: Vec<Region> = merged
        .into_iter()
        .map(|(kind, b, score)| {
            let pad = 0.25 * h;
            let px = b.rect.inflate(pad, pad);
            let px = Rect::from_corners(
                px.x.max(0.0),
                px.y.max(0.0),
                px.right().min(pw),
                px.bottom().min(ph),
            );
            let mut region = Region::new(kind, transform.to_points(&px), 0);
            region.score = score;
            region.line_count = b.count;
            region.column = if kind == RegionKind::Body {
                column_of(&b.rect)
            } else {
                None
            };
            region
        })
        .collect();

    // 9. uncovered text: line blobs with no OCR word on them
    let words_px: Vec<Rect> = input
        .words
        .iter()
        .filter(|w| !w.text.is_empty())
        .map(|w| transform.to_pixels(&w.bbox))
        .collect();
    for l in &lines {
        if l.count < 3 {
            continue;
        }
        let covered = words_px.iter().any(|w| w.intersects(&l.rect));
        if !covered {
            report.uncovered_text.push(transform.to_points(&l.rect));
        }
    }
    // uncovered blobs that fall outside every region become Uncertain regions
    for r in report.uncovered_text.clone() {
        if !regions.iter().any(|g| r.overlap_fraction(&g.bbox) > 0.5) {
            let mut u = Region::new(RegionKind::Uncertain, r, 0);
            u.score = 0.3;
            u.line_count = 1;
            regions.push(u);
        }
    }

    // 10. side-note anchors: nearest body line by vertical centre
    let body_lines_pt: Vec<f64> = lines
        .iter()
        .filter(|l| {
            regions.iter().any(|g| {
                g.kind == RegionKind::Body
                    && transform.to_points(&l.rect).overlap_fraction(&g.bbox) > 0.5
            })
        })
        .map(|l| transform.to_points(&l.rect).center().y)
        .collect();
    for g in regions
        .iter_mut()
        .filter(|g| g.kind == RegionKind::Marginalia)
    {
        let cy = g.bbox.y + report.text_height_pt; // first line of the note
        g.anchor_y = body_lines_pt
            .iter()
            .copied()
            .min_by(|a, b| (a - cy).abs().partial_cmp(&(b - cy).abs()).unwrap());
    }

    // 11. reading order: class rank, then column, then top
    regions.sort_by(|a, b| {
        a.kind
            .rank()
            .cmp(&b.kind.rank())
            .then(a.column.unwrap_or(0).cmp(&b.column.unwrap_or(0)))
            .then(a.bbox.y.partial_cmp(&b.bbox.y).unwrap())
            .then(a.bbox.x.partial_cmp(&b.bbox.x).unwrap())
    });
    for (i, r) in regions.iter_mut().enumerate() {
        r.order = i as u32;
    }

    // 12. coverage of OCR words and clipping
    report.words_total = input.words.iter().filter(|w| !w.text.is_empty()).count() as u32;
    report.words_uncovered = input
        .words
        .iter()
        .filter(|w| !w.text.is_empty())
        .filter(|w| {
            !regions
                .iter()
                .any(|g| g.kind.is_text() && w.bbox.overlap_fraction(&g.bbox) >= 0.5)
        })
        .count() as u32;
    let edge = transform.to_points(&Rect::new(0.0, 0.0, 2.0, 2.0)).w;
    report.clipped_regions = regions
        .iter()
        .filter(|g| {
            g.bbox.x <= edge
                || g.bbox.y <= edge
                || g.bbox.right() >= input.page_w - edge
                || g.bbox.bottom() >= input.page_h - edge
        })
        .count() as u32;
    report.gutters = gutter_px
        .iter()
        .map(|(a, b)| {
            (
                transform.to_points(&Rect::new(*a, 0.0, 0.0, 0.0)).x,
                transform.to_points(&Rect::new(*b, 0.0, 0.0, 0.0)).x,
            )
        })
        .collect();
    report.columns = (column_gutters.len() + 1) as u8;
    // Tesseract blocks spanning several of our regions (D-11 evidence)
    report.segmentation_disagreements = input
        .blocks
        .iter()
        .filter(|b| {
            regions
                .iter()
                .filter(|g| g.kind.is_text() && b.bbox.overlap_fraction(&g.bbox) >= 0.2)
                .count()
                >= 2
        })
        .count() as u32;
    if report.words_uncovered as f64 > 0.05 * report.words_total.max(1) as f64 {
        report.warnings.push(format!(
            "{} of {} words fall outside every region",
            report.words_uncovered, report.words_total
        ));
    }
    if !report.uncovered_text.is_empty() {
        report.warnings.push(format!(
            "{} text-like areas have no recognized words",
            report.uncovered_text.len()
        ));
    }
    let _ = input.lines;
    LayoutOutput {
        regions,
        report,
        algorithm: ALGORITHM.into(),
    }
}

pub const ALGORITHM: &str = "sbwb-layout/cc-smear/1";

#[cfg(test)]
mod tests {
    use super::*;
    use sbwb_core::PageIndex;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/hough-1839-vol1/pages-001-110.pdf")
    }

    fn page(idx: u32) -> Option<(GrayImage, Transform, sbwb_ocr::OcrOutput, f64, f64)> {
        let pdfium = sbwb_pdf::default_pdfium_dir();
        let tessdata = sbwb_ocr::default_tessdata_root();
        if !pdfium.join("pdfium.dll").exists() || !tessdata.join("fast/eng.traineddata").exists() {
            eprintln!("skipping: deps missing");
            return None;
        }
        let r = sbwb_pdf::Renderer::new(&pdfium).unwrap();
        let hi = r
            .render(
                &fixture(),
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(idx), 300),
            )
            .unwrap();
        let ocr = sbwb_ocr::Engine::new(&tessdata)
            .recognize(
                &hi.image,
                &hi.transform,
                &sbwb_ocr::OcrSettings {
                    model: sbwb_ocr::ModelPack::EngFast,
                    ..Default::default()
                },
            )
            .unwrap();
        let lo = r
            .render(
                &fixture(),
                None,
                &sbwb_pdf::RenderRequest::page_at_dpi(PageIndex(idx), crate::ANALYSIS_DPI),
            )
            .unwrap();
        Some((
            image::imageops::grayscale(&lo.image),
            lo.transform,
            ocr,
            lo.page_size.0,
            lo.page_size.1,
        ))
    }

    fn describe(out: &LayoutOutput) -> String {
        out.regions
            .iter()
            .map(|r| format!("{}:{}({}l)", r.order, r.kind.label(), r.line_count))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn body_page_with_side_notes() {
        let Some((gray, t, ocr, w, h)) = page(47) else {
            return;
        };
        let input = LayoutInput {
            page_w: w,
            page_h: h,
            words: &ocr.words,
            lines: &ocr.lines,
            blocks: &ocr.blocks,
        };
        let out = analyze(&gray, &t, &input, &AnalysisSettings::default());
        eprintln!("p48: {}\nreport: {:?}", describe(&out), out.report);
        let bodies: Vec<_> = out
            .regions
            .iter()
            .filter(|r| r.kind == RegionKind::Body)
            .collect();
        assert_eq!(
            bodies.len(),
            1,
            "expected one body region: {}",
            describe(&out)
        );
        assert!(bodies[0].line_count >= 20);
        let marg = out
            .regions
            .iter()
            .filter(|r| r.kind == RegionKind::Marginalia)
            .count();
        assert!(marg >= 2, "expected side notes: {}", describe(&out));
        assert!(
            out.regions.iter().any(|r| r.kind == RegionKind::Header),
            "{}",
            describe(&out)
        );
        assert!(
            out.regions.iter().any(|r| r.kind == RegionKind::Footnote),
            "{}",
            describe(&out)
        );
        // side notes sit left of the body
        for m in out
            .regions
            .iter()
            .filter(|r| r.kind == RegionKind::Marginalia)
        {
            assert!(
                m.bbox.right() <= bodies[0].bbox.x + 2.0,
                "note overlaps body"
            );
            assert!(m.anchor_y.is_some());
        }
        assert!(
            out.report.words_uncovered as f64 <= 0.05 * out.report.words_total as f64,
            "{:?}",
            out.report
        );
        assert_eq!(out.report.columns, 1);
    }

    #[test]
    fn another_body_page_is_consistent() {
        let Some((gray, t, ocr, w, h)) = page(60) else {
            return;
        };
        let input = LayoutInput {
            page_w: w,
            page_h: h,
            words: &ocr.words,
            lines: &ocr.lines,
            blocks: &ocr.blocks,
        };
        let out = analyze(&gray, &t, &input, &AnalysisSettings::default());
        eprintln!("p61: {}\nreport: {:?}", describe(&out), out.report);
        assert!(
            out.regions
                .iter()
                .filter(|r| r.kind == RegionKind::Body)
                .count()
                >= 1
        );
        assert!(
            out.report.words_uncovered as f64 <= 0.08 * out.report.words_total as f64,
            "{:?}",
            out.report
        );
        // reading order: header before body before footnotes
        let pos = |k: RegionKind| out.regions.iter().position(|r| r.kind == k);
        if let (Some(h), Some(b)) = (pos(RegionKind::Header), pos(RegionKind::Body)) {
            assert!(h < b);
        }
    }

    #[test]
    fn sparse_or_blank_pages_do_not_panic() {
        for idx in [0u32, 5, 10] {
            let Some((gray, t, ocr, w, h)) = page(idx) else {
                return;
            };
            let input = LayoutInput {
                page_w: w,
                page_h: h,
                words: &ocr.words,
                lines: &ocr.lines,
                blocks: &ocr.blocks,
            };
            let out = analyze(&gray, &t, &input, &AnalysisSettings::default());
            eprintln!("p{}: {}", idx + 1, describe(&out));
        }
    }
}
