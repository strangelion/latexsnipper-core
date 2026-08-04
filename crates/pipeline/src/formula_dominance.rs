//! Formula-dominant fast path.
//!
//! When a page contains formula detections, all of them are isolated/display
//! formulae, the formula regions cover the real ink above a versioned
//! threshold, and there is no significant text ink outside the formulae, the
//! pipeline skips TextDetection and TextRecognition entirely and runs
//! whole-image FormulaRecognition, producing a single FormulaBlock.
//!
//! All thresholds come from a versioned policy so decisions are reproducible
//! and never hard-coded at call sites.

use latexsnipper_ast::Rect;
use latexsnipper_image::SnipperImage;
use serde::{Deserialize, Serialize};

/// Versioned policy for the formula-dominant fast path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaDominancePolicy {
    pub version: String,
    /// Minimum number of formula boxes before the fast path is considered.
    pub min_formula_boxes: usize,
    /// Minimum ratio of ink covered by formula regions (relative to total ink).
    pub min_ink_coverage: f32,
    /// Maximum ratio of ink outside formula regions that is tolerated.
    pub max_outside_formula_ink: f32,
    /// Minimum average confidence of formula detections.
    pub min_formula_confidence: f32,
    /// Margin (px) around each formula rect when computing ink coverage.
    pub ink_margin: f32,
}

impl Default for FormulaDominancePolicy {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            min_formula_boxes: 1,
            min_ink_coverage: 0.75,
            max_outside_formula_ink: 0.05,
            min_formula_confidence: 0.5,
            ink_margin: 2.0,
        }
    }
}

/// A formula box plus its kind (isolated/display vs inline).
#[derive(Debug, Clone, Copy)]
pub struct FormulaBoxInput {
    pub rect: Rect,
    /// True when the formula is isolated or display (not inline).
    pub isolated: bool,
    pub confidence: f32,
}

/// The decision produced by the fast-path check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaDominanceDecision {
    pub formula_boxes: usize,
    pub isolated_only: bool,
    pub ink_coverage: f32,
    pub outside_formula_ink: f32,
    pub threshold_version: String,
    pub dominant: bool,
}

/// Check whether the image is formula-dominant.
///
/// Returns `Some(decision)` when the input is eligible for the fast path
/// (formula boxes present, all isolated/display); `None` when no formula
/// detection exists at all. The decision's `dominant` field is authoritative.
pub fn decide_formula_dominance(
    image: &SnipperImage,
    formula_boxes: &[FormulaBoxInput],
    policy: &FormulaDominancePolicy,
) -> Option<FormulaDominanceDecision> {
    if formula_boxes.is_empty() {
        return None;
    }

    let isolated_only = formula_boxes.iter().all(|f| f.isolated);
    let below_confidence = formula_boxes
        .iter()
        .any(|f| f.confidence < policy.min_formula_confidence);
    if !isolated_only || below_confidence {
        return Some(FormulaDominanceDecision {
            formula_boxes: formula_boxes.len(),
            isolated_only,
            ink_coverage: 0.0,
            outside_formula_ink: 1.0,
            threshold_version: policy.version.clone(),
            dominant: false,
        });
    }

    let (total_ink, covered_ink, outside_ink) =
        compute_ink_stats(image, formula_boxes, policy.ink_margin);

    let ink_coverage = if total_ink > 0.0 {
        covered_ink / total_ink
    } else {
        0.0
    };
    let outside_formula_ink = if total_ink > 0.0 {
        outside_ink / total_ink
    } else {
        0.0
    };

    let dominant = formula_boxes.len() >= policy.min_formula_boxes
        && ink_coverage >= policy.min_ink_coverage
        && outside_formula_ink <= policy.max_outside_formula_ink;

    Some(FormulaDominanceDecision {
        formula_boxes: formula_boxes.len(),
        isolated_only,
        ink_coverage,
        outside_formula_ink,
        threshold_version: policy.version.clone(),
        dominant,
    })
}

/// Compute total / covered / outside ink pixel ratios.
/// A pixel counts as ink when its luminance is below the image background
/// estimate (dark-ink assumption, matching the OCR ink mask).
fn compute_ink_stats(
    image: &SnipperImage,
    formula_boxes: &[FormulaBoxInput],
    margin: f32,
) -> (f32, f32, f32) {
    let w = image.width();
    let h = image.height();
    let channels = image.bytes_per_pixel();
    if w == 0 || h == 0 {
        return (0.0, 0.0, 0.0);
    }

    // Background estimate: 95th percentile luminance (lightest stable color).
    // Keep the raw luminance grid for spatial ink checks; sort a copy for the
    // percentile so pixel positions are preserved.
    let mut lums: Vec<f32> = Vec::with_capacity((w as usize) * (h as usize));
    for y in 0..h {
        for x in 0..w {
            let px = image.get_pixel(x, y);
            let (r, g, b) = match channels {
                1 => (px[0] as f32, px[0] as f32, px[0] as f32),
                _ => (px[0] as f32, px[1] as f32, px[2] as f32),
            };
            lums.push(0.299 * r + 0.587 * g + 0.114 * b);
        }
    }
    let mut sorted = lums.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95 = sorted[((sorted.len() as f32) * 0.95) as usize];
    let threshold = (p95 - 28.0).clamp(80.0, 245.0);

    // Expanded formula masks (capped at image bounds).
    let masks: Vec<(u32, u32, u32, u32)> = formula_boxes
        .iter()
        .map(|f| {
            let x0 = ((f.rect.x - margin).floor().max(0.0)) as u32;
            let y0 = ((f.rect.y - margin).floor().max(0.0)) as u32;
            let x1 = ((f.rect.right() + margin).ceil().min(w as f32)) as u32;
            let y1 = ((f.rect.bottom() + margin).ceil().min(h as f32)) as u32;
            (x0.min(x1), y0.min(y1), x1, y1)
        })
        .collect();

    let mut total = 0.0f32;
    let mut covered = 0.0f32;
    let mut outside = 0.0f32;

    for y in 0..h {
        for x in 0..w {
            let lum = lums[y as usize * w as usize + x as usize];
            if lum >= threshold {
                continue; // background pixel
            }
            total += 1.0;
            let inside = masks
                .iter()
                .any(|&(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
            if inside {
                covered += 1.0;
            } else {
                outside += 1.0;
            }
        }
    }

    (total, covered, outside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray_image(w: u32, h: u32, bg: u8) -> SnipperImage {
        SnipperImage::new(
            w,
            h,
            latexsnipper_image::color::PixelFormat::Rgb,
            (0..w * h).flat_map(|_| [bg, bg, bg]).collect(),
        )
    }

    /// Paint a dark (ink) rect into the image.
    fn paint_ink(img: &mut SnipperImage, x0: u32, y0: u32, x1: u32, y1: u32) {
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
    fn test_no_formula_returns_none() {
        let img = gray_image(100, 100, 255);
        let decision = decide_formula_dominance(&img, &[], &FormulaDominancePolicy::default());
        assert!(decision.is_none());
    }

    #[test]
    fn test_dominant_isolated_formula() {
        let mut img = gray_image(100, 100, 255);
        // Formula ink covers most of the image.
        paint_ink(&mut img, 10, 10, 90, 90);
        let boxes = vec![FormulaBoxInput {
            rect: Rect::new(10.0, 10.0, 80.0, 80.0),
            isolated: true,
            confidence: 0.95,
        }];
        let decision =
            decide_formula_dominance(&img, &boxes, &FormulaDominancePolicy::default()).unwrap();
        assert!(decision.dominant);
        assert!(decision.isolated_only);
        assert!(decision.ink_coverage >= 0.75);
        assert!(decision.outside_formula_ink <= 0.05);
        assert_eq!(decision.threshold_version, "v1");
    }

    #[test]
    fn test_inline_formula_not_dominant() {
        let img = gray_image(100, 100, 255);
        let boxes = vec![FormulaBoxInput {
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            isolated: false,
            confidence: 0.95,
        }];
        let decision =
            decide_formula_dominance(&img, &boxes, &FormulaDominancePolicy::default()).unwrap();
        assert!(!decision.dominant);
        assert!(!decision.isolated_only);
    }

    #[test]
    fn test_low_coverage_not_dominant() {
        let mut img = gray_image(100, 100, 255);
        // Large text ink block; the formula box only covers a small part.
        paint_ink(&mut img, 20, 20, 80, 80);
        let boxes = vec![FormulaBoxInput {
            rect: Rect::new(30.0, 30.0, 20.0, 20.0),
            isolated: true,
            confidence: 0.95,
        }];
        let decision =
            decide_formula_dominance(&img, &boxes, &FormulaDominancePolicy::default()).unwrap();
        assert!(!decision.dominant);
        assert!(decision.ink_coverage < 0.75);
    }

    #[test]
    fn test_outside_text_ink_not_dominant() {
        let mut img = gray_image(100, 100, 255);
        // Formula box plus significant text ink outside it.
        paint_ink(&mut img, 10, 10, 60, 60); // formula
        paint_ink(&mut img, 20, 80, 90, 85); // text line outside
        let boxes = vec![FormulaBoxInput {
            rect: Rect::new(10.0, 10.0, 50.0, 50.0),
            isolated: true,
            confidence: 0.95,
        }];
        let decision =
            decide_formula_dominance(&img, &boxes, &FormulaDominancePolicy::default()).unwrap();
        assert!(!decision.dominant);
        assert!(decision.outside_formula_ink > 0.05);
    }
}
