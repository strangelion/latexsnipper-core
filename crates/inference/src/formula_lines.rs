/// Formula line splitting and grouping.
/// Ported from LaTeXSnipper's formula_lines.py.
///
/// Segmentation is classification-aware: matrices, cases and compact
/// fractions are recognized as a whole by default, and only multi-line
/// evidence above the versioned threshold splits the image. Wide single
/// lines may be split horizontally along stable whitespace gaps. The legacy
/// `split_formula_line_groups` entry point remains as a compatibility layer.
use latexsnipper_image::SnipperImage;
use serde::{Deserialize, Serialize};

/// A cropped region of a formula line.
#[derive(Debug, Clone)]
pub struct FormulaLineCrop {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A group of formula line crops that should be recognized together.
#[derive(Debug, Clone)]
pub struct FormulaLineGroup {
    pub crops: Vec<FormulaLineCrop>,
}

/// How a formula image was classified before segmentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaSegmentationClass {
    /// Recognized as a single whole image (default).
    WholeImage,
    /// Multi-line derivation with enough evidence to split.
    MultiLine,
    /// Wide single line split horizontally along stable gaps.
    WideSingleLine,
    /// Matrix-like structure — recognized whole, never mechanically split.
    MatrixLike,
    /// Cases/environment with left brace — recognized whole.
    CasesLike,
    /// Compact fraction with a long horizontal rule — recognized whole.
    FractionLike,
    /// Classification could not be determined with confidence.
    Ambiguous,
}

/// Versioned policy controlling formula segmentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaSegmentPolicy {
    pub version: String,
    /// Minimum number of row bands before an image is treated as multi-line.
    pub min_multiline_bands: usize,
    /// Minimum band height for a row to be a real content row.
    pub min_band_height: usize,
    /// Minimum number of column gaps to classify as matrix-like.
    pub min_matrix_columns: usize,
    /// Minimum stable whitespace gap ratio (relative to width) for wide-line splits.
    pub min_gap_ratio: f32,
    /// Aspect ratio (width / height) above which a single band is wide.
    pub wide_aspect_ratio: f32,
    /// Longest-run fraction rule: ratio of horizontal ink run to width.
    pub fraction_rule_ratio: f32,
}

impl Default for FormulaSegmentPolicy {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            min_multiline_bands: 2,
            min_band_height: 6,
            min_matrix_columns: 3,
            min_gap_ratio: 0.06,
            wide_aspect_ratio: 3.2,
            fraction_rule_ratio: 0.35,
        }
    }
}

/// A segmentation plan: how the formula image should be recognized.
#[derive(Debug, Clone)]
pub struct FormulaSegmentPlan {
    pub groups: Vec<FormulaLineGroup>,
    pub classification: FormulaSegmentationClass,
    pub diagnostics: Vec<String>,
}

/// Compatibility layer: split a formula image into line groups for
/// recognition. Kept for existing callers; new code should use
/// [`plan_formula_segmentation`].
pub fn split_formula_line_groups(image: &SnipperImage) -> Vec<FormulaLineGroup> {
    plan_formula_segmentation(image).groups
}

/// Build a classification-aware segmentation plan for a formula image.
///
/// - matrices, cases and compact fractions are recognized whole by default;
/// - only sufficient multi-line evidence splits the image into rows;
/// - wide single lines may be split horizontally along stable gaps;
/// - diagnostics record every classification decision.
pub fn plan_formula_segmentation(image: &SnipperImage) -> FormulaSegmentPlan {
    let policy = FormulaSegmentPolicy::default();
    let w = image.width() as usize;
    let h = image.height() as usize;
    let mut diagnostics = Vec::new();

    if h < 24 || w < 12 {
        diagnostics.push("image too small for segmentation".into());
        return FormulaSegmentPlan {
            groups: vec![FormulaLineGroup {
                crops: vec![FormulaLineCrop {
                    pixels: image.pixels().to_vec(),
                    width: image.width(),
                    height: image.height(),
                }],
            }],
            classification: FormulaSegmentationClass::Ambiguous,
            diagnostics,
        };
    }

    let gray = to_grayscale(image);
    let mask = ink_mask(&gray, w, h);
    let bands = row_bands(&mask, w, h);
    let content_bands: Vec<(usize, usize)> = bands
        .iter()
        .copied()
        .filter(|(top, bottom)| bottom - top + 1 >= policy.min_band_height)
        .collect();

    // Fraction rule: a long horizontal ink run spanning a large part of the
    // width indicates a fraction bar → recognize whole.
    if has_long_horizontal_rule(&mask, w, h, policy.fraction_rule_ratio) {
        diagnostics.push("fraction bar detected — recognize whole".into());
        return whole_image(image, FormulaSegmentationClass::FractionLike, diagnostics);
    }

    // Matrix-like: multiple stable vertical gaps partition the ink into 3+
    // columns while row count is low → recognize whole.
    let column_gaps = stable_column_gaps(&mask, w, h);
    if column_gaps >= policy.min_matrix_columns {
        diagnostics.push(format!(
            "matrix-like structure ({} column gaps) — recognize whole",
            column_gaps
        ));
        return whole_image(image, FormulaSegmentationClass::MatrixLike, diagnostics);
    }

    // Cases-like: a strong vertical stroke near the left edge.
    if has_left_brace_stroke(&mask, w, h) {
        diagnostics.push("cases-like left brace detected — recognize whole".into());
        return whole_image(image, FormulaSegmentationClass::CasesLike, diagnostics);
    }

    if content_bands.len() >= policy.min_multiline_bands {
        diagnostics.push(format!(
            "multi-line evidence ({} bands) — split rows",
            content_bands.len()
        ));
        let mut groups = Vec::new();
        for (top, bottom) in &content_bands {
            if let Some(crop) = crop_line(image, *top, *bottom) {
                groups.push(FormulaLineGroup { crops: vec![crop] });
            }
        }
        return FormulaSegmentPlan {
            groups,
            classification: FormulaSegmentationClass::MultiLine,
            diagnostics,
        };
    }

    // Wide single line: split horizontally along stable whitespace gaps so
    // the recognizer sees narrower crops (keeps long derivations readable).
    let aspect = if h > 0 { w as f32 / h as f32 } else { 0.0 };
    if aspect >= policy.wide_aspect_ratio {
        let gaps = wide_line_gaps(&mask, w, h, policy.min_gap_ratio);
        if !gaps.is_empty() {
            diagnostics.push(format!(
                "wide single line (aspect {aspect:.1}) — split at {} gaps",
                gaps.len()
            ));
            let crops = crop_horizontal_segments(image, &gaps);
            return FormulaSegmentPlan {
                groups: vec![FormulaLineGroup { crops }],
                classification: FormulaSegmentationClass::WideSingleLine,
                diagnostics,
            };
        }
    }

    diagnostics.push("no split evidence — whole image".into());
    whole_image(image, FormulaSegmentationClass::WholeImage, diagnostics)
}

fn whole_image(
    image: &SnipperImage,
    classification: FormulaSegmentationClass,
    diagnostics: Vec<String>,
) -> FormulaSegmentPlan {
    FormulaSegmentPlan {
        groups: vec![FormulaLineGroup {
            crops: vec![FormulaLineCrop {
                pixels: image.pixels().to_vec(),
                width: image.width(),
                height: image.height(),
            }],
        }],
        classification,
        diagnostics,
    }
}

/// True when a horizontal ink run covers `ratio` of the image width in at
/// least one row (a fraction bar).
/// True when a thin, isolated horizontal ink run covers `ratio` of the image
/// width in at least one row. A fraction bar is a thin line (bounded by
/// `max_rule_thickness` rows) with no ink outside that thin span; a solid
/// text block is thicker, so it is never mistaken for a rule.
fn has_long_horizontal_rule(mask: &[bool], w: usize, h: usize, ratio: f32) -> bool {
    let target = (w as f32 * ratio) as usize;
    let max_rule_thickness = 4usize.max(h / 24);
    for y in 0..h {
        let mut row_run = 0usize;
        for x in 0..w {
            if mask[y * w + x] {
                row_run += 1;
                if row_run >= target {
                    // Measure the full vertical thickness of the stroke at
                    // this column (up and down). Thin = fraction rule.
                    let mut top = y;
                    while top > 0 && mask[(top - 1) * w + x] {
                        top -= 1;
                    }
                    let mut bottom = y;
                    while bottom + 1 < h && mask[(bottom + 1) * w + x] {
                        bottom += 1;
                    }
                    if bottom - top < max_rule_thickness {
                        return true;
                    }
                    row_run = 0;
                }
            } else {
                row_run = 0;
            }
        }
    }
    false
}

/// Count stable vertical whitespace columns that partition the ink into
/// several column groups (matrix-like evidence).
fn stable_column_gaps(mask: &[bool], w: usize, h: usize) -> usize {
    let mut column_has_ink = vec![false; w];
    for (x, col) in column_has_ink.iter_mut().enumerate() {
        *col = (0..h).any(|y| mask[y * w + x]);
    }
    // Count runs of empty columns (gap >= 2 columns wide counts as stable).
    let mut gaps = 0usize;
    let mut run = 0usize;
    for has_ink in &column_has_ink {
        if *has_ink {
            if run >= 2 {
                gaps += 1;
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    gaps
}

/// Detect a cases-like left brace: a tall region of ink in the left 10% of
/// the image spanning a large share of the height.
/// Detect a cases-like left brace: a thin vertical stroke (bounded column
/// span) inside the left 10% of the image that covers most of the height.
/// A brace is narrow and tall; a text row's left edge is short, so it is
/// never mistaken for a brace.
fn has_left_brace_stroke(mask: &[bool], w: usize, h: usize) -> bool {
    let left_limit = ((w as f32) * 0.1).max(2.0) as usize;
    let max_stroke_width = 4usize;
    // For every column in the left band, measure the tallest contiguous ink
    // run. A brace column spans most of the height.
    let min_height = (h as f32 * 0.6) as usize;
    for x in 0..left_limit.min(w) {
        let mut best = 0usize;
        let mut cur = 0usize;
        for y in 0..h {
            if mask[y * w + x] {
                cur += 1;
                best = best.max(cur);
            } else {
                cur = 0;
            }
        }
        if best >= min_height {
            // Confirm the stroke stays thin horizontally.
            let mut width = 1usize;
            for dx in 1..max_stroke_width {
                if x + dx < w && (0..h).any(|y| mask[y * w + (x + dx)]) {
                    width += 1;
                }
            }
            if width <= max_stroke_width {
                return true;
            }
        }
    }
    false
}

/// Stable whitespace gaps (columns with no ink) used to split a wide line.
/// Returns the gap x-positions (centers of empty runs).
fn wide_line_gaps(mask: &[bool], w: usize, h: usize, min_gap_ratio: f32) -> Vec<usize> {
    let min_gap = (w as f32 * min_gap_ratio) as usize;
    let mut column_has_ink = vec![false; w];
    for (x, col) in column_has_ink.iter_mut().enumerate() {
        *col = (0..h).any(|y| mask[y * w + x]);
    }
    let mut gaps = Vec::new();
    let mut run = 0usize;
    let mut run_start = 0usize;
    for (x, has_ink) in column_has_ink.iter().enumerate() {
        if *has_ink {
            if run >= min_gap {
                gaps.push(run_start + run / 2);
            }
            run = 0;
        } else {
            if run == 0 {
                run_start = x;
            }
            run += 1;
        }
    }
    gaps
}

/// Crop horizontal segments of a wide line between the given gaps.
fn crop_horizontal_segments(image: &SnipperImage, gaps: &[usize]) -> Vec<FormulaLineCrop> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let mut segments = Vec::new();
    let mut start = 0usize;
    for &gap in gaps {
        if gap > start {
            if let Some(crop) = crop_rect(image, start, 0, gap - start, h) {
                segments.push(crop);
            }
        }
        start = gap;
    }
    if start < w {
        if let Some(crop) = crop_rect(image, start, 0, w - start, h) {
            segments.push(crop);
        }
    }
    segments
}

fn crop_rect(
    image: &SnipperImage,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Option<FormulaLineCrop> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    if x >= w || y >= h || width == 0 || height == 0 || x + width > w || y + height > h {
        return None;
    }
    let pixels = image.pixels();
    let mut crop_pixels = Vec::with_capacity(width * height * 3);
    for yy in y..y + height {
        for xx in x..x + width {
            let src_idx = (yy * w + xx) * 3;
            if src_idx + 2 < pixels.len() {
                crop_pixels.push(pixels[src_idx]);
                crop_pixels.push(pixels[src_idx + 1]);
                crop_pixels.push(pixels[src_idx + 2]);
            }
        }
    }
    Some(FormulaLineCrop {
        pixels: crop_pixels,
        width: width as u32,
        height: height as u32,
    })
}

fn to_grayscale(image: &SnipperImage) -> Vec<f32> {
    let pixels = image.pixels();
    let w = image.width() as usize;
    let h = image.height() as usize;
    let mut gray = vec![0.0f32; w * h];

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 3;
            if idx + 2 < pixels.len() {
                gray[y * w + x] = 0.299 * pixels[idx] as f32
                    + 0.587 * pixels[idx + 1] as f32
                    + 0.114 * pixels[idx + 2] as f32;
            }
        }
    }
    gray
}

fn ink_mask(gray: &[f32], w: usize, h: usize) -> Vec<bool> {
    // Calculate background threshold
    let mut sorted: Vec<f32> = gray.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = (sorted.len() as f32 * 0.95) as usize;
    let background = sorted[p95_idx.min(sorted.len() - 1)];
    let threshold = background - 28.0;
    let threshold = threshold.clamp(80.0, 245.0);

    let mut mask = vec![false; w * h];
    let border = 1.max(w.min(h) / 80);

    for y in 0..h {
        for x in 0..w {
            if y < border || y >= h - border || x < border || x >= w - border {
                continue;
            }
            mask[y * w + x] = gray[y * w + x] < threshold;
        }
    }

    mask
}

fn row_bands(mask: &[bool], w: usize, h: usize) -> Vec<(usize, usize)> {
    let row_threshold = 3.max((w as f32 * 0.006) as usize);

    let mut row_has_ink = Vec::new();
    for y in 0..h {
        let count = (0..w).filter(|&x| mask[y * w + x]).count();
        row_has_ink.push(count >= row_threshold);
    }

    let mut bands = Vec::new();
    let mut start: Option<usize> = None;

    for (y, &has_ink) in row_has_ink.iter().enumerate() {
        if has_ink && start.is_none() {
            start = Some(y);
        } else if !has_ink && start.is_some() {
            bands.push((start.unwrap(), y - 1));
            start = None;
        }
    }
    if let Some(s) = start {
        bands.push((s, h - 1));
    }

    // Merge close bands
    let max_gap = 3.max(14.min((h as f32 * 0.018) as usize));
    merge_close_bands(&mut bands, max_gap);

    // Filter bands that look like formula rows
    bands.retain(|&(top, bottom)| {
        let band_height = bottom - top + 1;
        band_height >= 4 && band_height <= h / 2
    });

    bands
}

fn merge_close_bands(bands: &mut Vec<(usize, usize)>, max_gap: usize) {
    if bands.len() < 2 {
        return;
    }

    let mut merged = Vec::new();
    let mut current = bands[0];

    for &(top, bottom) in &bands[1..] {
        if top <= current.1 + max_gap {
            current.1 = bottom;
        } else {
            merged.push(current);
            current = (top, bottom);
        }
    }
    merged.push(current);

    *bands = merged;
}

fn crop_line(image: &SnipperImage, top: usize, bottom: usize) -> Option<FormulaLineCrop> {
    let w = image.width() as usize;
    let h = image.height() as usize;

    if top >= h || bottom >= h || top >= bottom {
        return None;
    }

    let line_height = bottom - top + 1;
    let pixels = image.pixels();
    let mut crop_pixels = Vec::with_capacity(w * line_height * 3);

    for y in top..=bottom {
        for x in 0..w {
            let src_idx = (y * w + x) * 3;
            if src_idx + 2 < pixels.len() {
                crop_pixels.push(pixels[src_idx]);
                crop_pixels.push(pixels[src_idx + 1]);
                crop_pixels.push(pixels[src_idx + 2]);
            }
        }
    }

    Some(FormulaLineCrop {
        pixels: crop_pixels,
        width: w as u32,
        height: line_height as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_image(w: u32, h: u32) -> SnipperImage {
        SnipperImage::new(
            w,
            h,
            latexsnipper_image::color::PixelFormat::Rgb,
            vec![255u8; (w * h * 3) as usize],
        )
    }

    fn paint_rect(img: &mut SnipperImage, x0: u32, y0: u32, x1: u32, y1: u32) {
        let w = img.width();
        for y in y0..y1 {
            for x in x0..x1 {
                let off = (y * w + x) as usize * 3;
                img.pixels_mut()[off] = 0;
                img.pixels_mut()[off + 1] = 0;
                img.pixels_mut()[off + 2] = 0;
            }
        }
    }

    #[test]
    fn test_ink_mask_basic() {
        let gray = vec![255.0; 100]; // All white
        let mask = ink_mask(&gray, 10, 10);
        assert!(mask.iter().all(|&m| !m)); // No ink
    }

    #[test]
    fn test_row_bands_empty() {
        let mask = vec![false; 100];
        let bands = row_bands(&mask, 10, 10);
        assert!(bands.is_empty());
    }

    #[test]
    fn test_merge_close_bands() {
        let mut bands = vec![(0, 5), (7, 12), (20, 25)];
        merge_close_bands(&mut bands, 3);
        assert_eq!(bands.len(), 2); // First two should merge
    }

    #[test]
    fn test_single_line_is_whole_image() {
        let mut img = make_image(120, 40);
        paint_rect(&mut img, 10, 12, 110, 28);
        let plan = plan_formula_segmentation(&img);
        assert_eq!(plan.classification, FormulaSegmentationClass::WholeImage);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].crops.len(), 1);
    }

    #[test]
    fn test_multiline_splits_rows() {
        let mut img = make_image(120, 90);
        paint_rect(&mut img, 10, 5, 110, 20); // row 1
        paint_rect(&mut img, 10, 40, 110, 55); // row 2
        paint_rect(&mut img, 10, 70, 110, 85); // row 3
        let plan = plan_formula_segmentation(&img);
        assert_eq!(plan.classification, FormulaSegmentationClass::MultiLine);
        assert_eq!(plan.groups.len(), 3);
    }

    #[test]
    fn test_fraction_bar_recognized_whole() {
        let mut img = make_image(120, 60);
        paint_rect(&mut img, 5, 28, 115, 30); // long fraction rule
        let plan = plan_formula_segmentation(&img);
        assert_eq!(plan.classification, FormulaSegmentationClass::FractionLike);
        assert_eq!(plan.groups.len(), 1);
    }

    #[test]
    fn test_matrix_like_recognized_whole() {
        let mut img = make_image(150, 60);
        // Three column groups separated by stable gaps.
        paint_rect(&mut img, 10, 10, 30, 50);
        paint_rect(&mut img, 60, 10, 80, 50);
        paint_rect(&mut img, 110, 10, 130, 50);
        let plan = plan_formula_segmentation(&img);
        assert_eq!(plan.classification, FormulaSegmentationClass::MatrixLike);
        assert_eq!(plan.groups.len(), 1);
    }

    #[test]
    fn test_cases_like_recognized_whole() {
        let mut img = make_image(120, 80);
        // Left brace stroke on the left edge.
        paint_rect(&mut img, 2, 10, 6, 70);
        paint_rect(&mut img, 20, 10, 100, 25);
        paint_rect(&mut img, 20, 55, 100, 70);
        let plan = plan_formula_segmentation(&img);
        assert_eq!(plan.classification, FormulaSegmentationClass::CasesLike);
        assert_eq!(plan.groups.len(), 1);
    }

    #[test]
    fn test_wide_single_line_splits_horizontally() {
        let mut img = make_image(400, 30);
        paint_rect(&mut img, 10, 10, 120, 20);
        paint_rect(&mut img, 280, 10, 390, 20);
        // Stable empty gap between x=120 and x=280.
        let plan = plan_formula_segmentation(&img);
        assert_eq!(
            plan.classification,
            FormulaSegmentationClass::WideSingleLine
        );
        assert_eq!(plan.groups.len(), 1);
        assert!(plan.groups[0].crops.len() >= 2);
    }

    #[test]
    fn test_legacy_splitter_is_compatible() {
        let mut img = make_image(120, 90);
        paint_rect(&mut img, 10, 5, 110, 20);
        paint_rect(&mut img, 10, 40, 110, 55);
        let groups = split_formula_line_groups(&img);
        assert_eq!(groups.len(), 2);
    }
}
