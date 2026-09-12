//! Parser for Tesseract's TSV output.
//!
//! Columns: level page_num block_num par_num line_num word_num left top
//! width height conf text. Level 2 = block, 4 = line, 5 = word.

use sbwb_core::{Rect, Transform};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrWord {
    pub block: u32,
    pub paragraph: u32,
    pub line: u32,
    pub index: u32,
    /// Bounding box in page points.
    pub bbox: Rect,
    /// Bounding box in bitmap pixels as reported by the engine.
    pub bbox_px: Rect,
    /// Native word confidence, 0-100 (OCR-01).
    pub confidence: f32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLine {
    pub block: u32,
    pub paragraph: u32,
    pub line: u32,
    pub bbox: Rect,
    pub word_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBlock {
    pub block: u32,
    pub bbox: Rect,
}

#[derive(Debug, Default, Clone)]
pub struct TsvPage {
    pub words: Vec<OcrWord>,
    pub lines: Vec<OcrLine>,
    pub blocks: Vec<OcrBlock>,
}

pub fn parse_tsv(tsv: &str, transform: &Transform) -> TsvPage {
    let mut page = TsvPage::default();
    for (i, row) in tsv.lines().enumerate() {
        if i == 0 && row.starts_with("level") {
            continue;
        }
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 11 {
            continue;
        }
        let num = |i: usize| cols[i].trim().parse::<f64>().unwrap_or(0.0);
        let level = num(0) as u32;
        let (block, par, line, word) = (num(2) as u32, num(3) as u32, num(4) as u32, num(5) as u32);
        let bbox_px = Rect::new(num(6), num(7), num(8), num(9));
        let bbox = transform.to_points(&bbox_px);
        match level {
            2 => page.blocks.push(OcrBlock { block, bbox }),
            4 => page.lines.push(OcrLine {
                block,
                paragraph: par,
                line,
                bbox,
                word_count: 0,
            }),
            5 => {
                let conf = num(10) as f32;
                let text = cols
                    .get(11)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if conf < 0.0 && text.is_empty() {
                    continue;
                }
                if let Some(l) = page
                    .lines
                    .iter_mut()
                    .rev()
                    .find(|l| l.block == block && l.paragraph == par && l.line == line)
                {
                    l.word_count += 1;
                }
                page.words.push(OcrWord {
                    block,
                    paragraph: par,
                    line,
                    index: word,
                    bbox,
                    bbox_px,
                    confidence: conf.max(0.0),
                    text,
                });
            }
            _ => {}
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
1\t1\t0\t0\t0\t0\t0\t0\t2000\t3000\t-1\t\n\
2\t1\t1\t0\t0\t0\t100\t100\t1800\t200\t-1\t\n\
3\t1\t1\t1\t0\t0\t100\t100\t1800\t200\t-1\t\n\
4\t1\t1\t1\t1\t0\t100\t100\t1800\t60\t-1\t\n\
5\t1\t1\t1\t1\t1\t100\t100\t300\t60\t96.5\tTarshish.\n\
5\t1\t1\t1\t1\t2\t420\t102\t200\t58\t64\tcom-\n";

    #[test]
    fn parses_levels_and_scales() {
        let t = Transform::for_dpi(300);
        let p = parse_tsv(SAMPLE, &t);
        assert_eq!(p.blocks.len(), 1);
        assert_eq!(p.lines.len(), 1);
        assert_eq!(p.words.len(), 2);
        assert_eq!(p.lines[0].word_count, 2);
        let w = &p.words[0];
        assert_eq!(w.text, "Tarshish.");
        assert!((w.confidence - 96.5).abs() < 1e-3);
        assert!((w.bbox.x - 100.0 * 72.0 / 300.0).abs() < 1e-9);
        assert_eq!(w.bbox_px.w, 300.0);
    }
}
