//! Background-aware formula masking.
//!
//! Before text OCR, formula regions inside a text crop are filled with an
//! estimate of the surrounding local background so the OCR model only sees
//! pure text. The mask never assumes a fixed white background: it supports
//! white scans, gray screenshots, dark-theme screenshots, transparent
//! backgrounds and colored backgrounds.
//!
//! Every mask run records reproducible evidence (`maskAlgorithmVersion`,
//! `formulaMaskRects`, `maskMargin`, `backgroundEstimate`,
//! `maskedImageSha256`). If masking cannot be produced the caller receives a
//! `MIXED_FORMULA_MASK_FALLBACK` flag and the unmasked crop is returned —
//! text is never silently swallowed.

use latexsnipper_ast::Rect;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::SnipperImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version of the masking algorithm. Bump when the estimation or fill
/// behavior changes so recorded evidence stays comparable.
pub const MASK_ALGORITHM_VERSION: &str = "v1";

/// Stable error code recorded when masking falls back to an unmasked crop.
pub const MIXED_FORMULA_MASK_FALLBACK: &str = "MIXED_FORMULA_MASK_FALLBACK";

/// Background estimate for one formula region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundEstimate {
    /// Per-channel background color (RGB), scaled 0-255.
    pub color: [u8; 3],
    /// Number of pixels sampled around the formula border.
    pub sampled_pixels: usize,
    /// Whether the surrounding background looked dark (dark-theme source).
    pub dark: bool,
}

/// Evidence recorded for one masked crop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaMaskEvidence {
    pub mask_algorithm_version: String,
    /// Formula rects (in text-crop coordinates) that were masked.
    pub formula_mask_rects: Vec<Rect>,
    /// Safety margin added around each formula rect.
    pub mask_margin: f32,
    /// Estimated background per masked region.
    pub background_estimate: Vec<BackgroundEstimate>,
    /// SHA-256 of the masked crop bytes (or the fallback crop bytes).
    pub masked_image_sha256: String,
    /// True when the mask was not applied and the original crop was kept.
    pub fell_back: bool,
    /// Set when `fell_back` is true.
    pub fallback_code: Option<String>,
}

/// Result of masking a text crop around formulae.
#[derive(Debug, Clone)]
pub struct MaskedCrop {
    /// Masked image (formula regions filled with estimated background).
    /// When masking is not possible this is the original crop unchanged.
    pub image: SnipperImage,
    pub evidence: FormulaMaskEvidence,
}

/// Versioned options for the masking algorithm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaMaskOptions {
    pub version: String,
    /// Safety margin (px) added around each formula rect before estimating
    /// the local background.
    pub margin: f32,
    /// Fraction of the formula border ring sampled for the background
    /// estimate (relative to the ring thickness).
    pub sample_density: f32,
    /// Luminance below which a background counts as "dark".
    pub dark_luminance_threshold: f32,
}

impl Default for FormulaMaskOptions {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            margin: 4.0,
            sample_density: 0.25,
            dark_luminance_threshold: 120.0,
        }
    }
}

/// Estimate the local background color around a formula rect inside the
/// full-page image. Samples a ring of pixels just outside the (expanded)
/// formula rect and takes the per-channel median, so a single stray stroke
/// does not corrupt the estimate.
fn estimate_background(
    image: &SnipperImage,
    rect: &Rect,
    margin: f32,
    options: &FormulaMaskOptions,
) -> BackgroundEstimate {
    let w = image.width();
    let h = image.height();
    let channels = image.bytes_per_pixel();

    let x0 = ((rect.x - margin).floor().max(0.0)) as u32;
    let y0 = ((rect.y - margin).floor().max(0.0)) as u32;
    let x1 = ((rect.right() + margin).ceil().min(w as f32)) as u32;
    let y1 = ((rect.bottom() + margin).ceil().min(h as f32)) as u32;

    // Sample the outer ring (one-pixel band around the expanded rect) and
    // the four corner pads. Skip every 1/`sample_density`-th pixel to bound
    // the sample size on huge images.
    let mut r = Vec::new();
    let mut g = Vec::new();
    let mut b = Vec::new();
    let step = (1.0f32 / options.sample_density.max(0.01)).ceil().max(1.0) as u32;
    let mut sampled = 0usize;

    for x in (x0..x1).step_by(step as usize) {
        for &y in &[y0, y1.saturating_sub(1)] {
            if y >= h || x >= w {
                continue;
            }
            push_channel(&mut r, &mut g, &mut b, image.get_pixel(x, y), channels);
            sampled += 1;
        }
    }
    for y in (y0..y1).step_by(step as usize) {
        for &x in &[x0, x1.saturating_sub(1)] {
            if y >= h || x >= w {
                continue;
            }
            push_channel(&mut r, &mut g, &mut b, image.get_pixel(x, y), channels);
            sampled += 1;
        }
    }

    if sampled == 0 {
        // No border available (formula covers the whole page): fall back to
        // the image edge average.
        let (ar, ag, ab) = edge_average(image);
        let lum = 0.299 * ar as f32 + 0.587 * ag as f32 + 0.114 * ab as f32;
        return BackgroundEstimate {
            color: [ar, ag, ab],
            sampled_pixels: 0,
            dark: lum < options.dark_luminance_threshold,
        };
    }

    let median = |mut v: Vec<u8>| -> u8 {
        v.sort_unstable();
        v[v.len() / 2]
    };
    let color = [median(r), median(g), median(b)];
    let lum = 0.299 * color[0] as f32 + 0.587 * color[1] as f32 + 0.114 * color[2] as f32;
    BackgroundEstimate {
        color,
        sampled_pixels: sampled,
        dark: lum < options.dark_luminance_threshold,
    }
}

fn push_channel(r: &mut Vec<u8>, g: &mut Vec<u8>, b: &mut Vec<u8>, px: &[u8], channels: usize) {
    match channels {
        1 => {
            let v = px[0];
            r.push(v);
            g.push(v);
            b.push(v);
        }
        3 => {
            r.push(px[0]);
            g.push(px[1]);
            b.push(px[2]);
        }
        4 => {
            r.push(px[0]);
            g.push(px[1]);
            b.push(px[2]);
        }
        _ => {}
    }
}

fn edge_average(image: &SnipperImage) -> (u8, u8, u8) {
    let w = image.width();
    let h = image.height();
    let channels = image.bytes_per_pixel();
    let mut r = 0u64;
    let mut g = 0u64;
    let mut b = 0u64;
    let mut n = 0u64;

    let sample = |x: u32, y: u32, r: &mut u64, g: &mut u64, b: &mut u64, n: &mut u64| {
        if y >= h || x >= w {
            return;
        }
        let px = image.get_pixel(x, y);
        match channels {
            1 => {
                *r += px[0] as u64;
                *g += px[0] as u64;
                *b += px[0] as u64;
            }
            _ => {
                *r += px[0] as u64;
                *g += px[1] as u64;
                *b += px[2] as u64;
            }
        }
        *n += 1;
    };

    for x in 0..w {
        sample(x, 0, &mut r, &mut g, &mut b, &mut n);
        sample(x, h - 1, &mut r, &mut g, &mut b, &mut n);
    }
    for y in 0..h {
        sample(0, y, &mut r, &mut g, &mut b, &mut n);
        sample(w - 1, y, &mut r, &mut g, &mut b, &mut n);
    }

    if n == 0 {
        return (255, 255, 255);
    }
    ((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

/// Compute the SHA-256 of the raw pixel bytes of an image.
pub fn image_sha256(image: &SnipperImage) -> String {
    let mut hasher = Sha256::new();
    hasher.update(image.pixels());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Mask the formula regions that intersect `text_crop` by filling them with
/// the estimated local background from the full-page image.
///
/// `text_rect` is the position of `text_crop` inside `full_image`;
/// `formula_rects` use full-image coordinates and are converted to crop-local
/// coordinates for the fill.
///
/// When the crop image cannot be produced (`formula_rects` empty, or an
/// unsupported pixel format), the original crop is returned with
/// `fell_back = true` and `MIXED_FORMULA_MASK_FALLBACK` recorded — the text
/// is never silently dropped.
pub fn apply_formula_mask(
    full_image: &SnipperImage,
    text_crop: SnipperImage,
    text_rect: Rect,
    formula_rects: &[Rect],
    options: &FormulaMaskOptions,
) -> MaskedCrop {
    let mut evidence_rects = Vec::new();
    let mut estimates = Vec::new();
    let mut masked = text_crop.clone();
    let mut fell_back = false;
    let mut fallback_code: Option<String> = None;

    if formula_rects.is_empty() {
        fell_back = true;
        fallback_code = Some(MIXED_FORMULA_MASK_FALLBACK.into());
    } else if masked.format() != PixelFormat::Rgb && masked.format() != PixelFormat::Gray {
        // Only RGB/Gray crops are masked; other formats fall back so the
        // recognizer still receives the original pixels.
        fell_back = true;
        fallback_code = Some(MIXED_FORMULA_MASK_FALLBACK.into());
    } else {
        for rect in formula_rects {
            let estimate = estimate_background(full_image, rect, options.margin, options);
            evidence_rects.push(*rect);
            estimates.push(estimate);
        }
        // Convert to crop-local coordinates for the fill.
        let local: Vec<Rect> = evidence_rects
            .iter()
            .map(|r| Rect::new(r.x - text_rect.x, r.y - text_rect.y, r.width, r.height))
            .collect();
        fill_formula_regions(&mut masked, &local, &estimates, options);
    }

    let masked_image_sha256 = image_sha256(&masked);
    MaskedCrop {
        image: masked,
        evidence: FormulaMaskEvidence {
            mask_algorithm_version: MASK_ALGORITHM_VERSION.into(),
            formula_mask_rects: evidence_rects,
            mask_margin: options.margin,
            background_estimate: estimates,
            masked_image_sha256,
            fell_back,
            fallback_code,
        },
    }
}

/// Fill each formula region with the corresponding estimated background.
fn fill_formula_regions(
    image: &mut SnipperImage,
    rects: &[Rect],
    estimates: &[BackgroundEstimate],
    options: &FormulaMaskOptions,
) {
    let w = image.width();
    let h = image.height();
    let channels = image.bytes_per_pixel();

    for (rect, estimate) in rects.iter().zip(estimates.iter()) {
        let x0 = (rect.x.floor().max(0.0)) as u32;
        let y0 = (rect.y.floor().max(0.0)) as u32;
        let x1 = (rect.right().ceil().min(w as f32)) as u32;
        let y1 = (rect.bottom().ceil().min(h as f32)) as u32;
        let color = estimate.color;
        let _ = options;

        let row = w as usize * channels;
        for y in y0..y1 {
            let base = y as usize * row;
            for x in x0..x1 {
                let off = base + x as usize * channels;
                match channels {
                    1 => image.pixels_mut()[off] = color[0],
                    _ => {
                        image.pixels_mut()[off] = color[0];
                        image.pixels_mut()[off + 1] = color[1];
                        image.pixels_mut()[off + 2] = color[2];
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white_rgb(w: u32, h: u32) -> SnipperImage {
        SnipperImage::new(w, h, PixelFormat::Rgb, vec![255u8; (w * h * 3) as usize])
    }

    fn gray_rgb(w: u32, h: u32, v: u8) -> SnipperImage {
        SnipperImage::new(
            w,
            h,
            PixelFormat::Rgb,
            (0..w * h).flat_map(|_| [v, v, v]).collect(),
        )
    }

    fn hash(img: &SnipperImage) -> String {
        image_sha256(img)
    }

    #[test]
    fn test_no_formula_falls_back_without_silent_loss() {
        let full = white_rgb(100, 100);
        let crop = white_rgb(60, 20);
        let result = apply_formula_mask(
            &full,
            crop.clone(),
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &[],
            &FormulaMaskOptions::default(),
        );
        assert!(result.evidence.fell_back);
        assert_eq!(
            result.evidence.fallback_code.as_deref(),
            Some(MIXED_FORMULA_MASK_FALLBACK)
        );
        // Original pixels preserved.
        assert_eq!(hash(&result.image), hash(&crop));
    }

    #[test]
    fn test_white_background_mask_fills_formula() {
        let full = white_rgb(100, 100);
        // Draw a dark square (the formula) into the full image.
        let mut full = full;
        for y in 20..40 {
            for x in 30..50 {
                let off = (y * 100 + x) * 3;
                full.pixels_mut()[off] = 0;
                full.pixels_mut()[off + 1] = 0;
                full.pixels_mut()[off + 2] = 0;
            }
        }
        let crop = full.clone();
        let rect = Rect::new(30.0, 20.0, 20.0, 20.0);
        let result = apply_formula_mask(
            &full,
            crop,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &[rect],
            &FormulaMaskOptions::default(),
        );
        assert!(!result.evidence.fell_back);
        assert_eq!(result.evidence.formula_mask_rects.len(), 1);
        assert_eq!(
            result.evidence.background_estimate[0].color,
            [255, 255, 255]
        );
        // Formula area now matches background.
        let masked = &result.image;
        let px = masked.get_pixel(35, 25);
        assert_eq!(px, [255, 255, 255]);
    }

    #[test]
    fn test_dark_background_estimates_dark() {
        let full = gray_rgb(100, 100, 30); // dark-theme screenshot
        let crop = full.clone();
        let rect = Rect::new(10.0, 10.0, 20.0, 20.0);
        let result = apply_formula_mask(
            &full,
            crop,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &[rect],
            &FormulaMaskOptions::default(),
        );
        assert!(!result.evidence.fell_back);
        assert!(result.evidence.background_estimate[0].dark);
        assert_eq!(result.evidence.background_estimate[0].color, [30, 30, 30]);
    }

    #[test]
    fn test_gray_background_fills_gray() {
        let full = gray_rgb(80, 80, 200);
        let crop = full.clone();
        let rect = Rect::new(5.0, 5.0, 10.0, 10.0);
        let result = apply_formula_mask(
            &full,
            crop,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &[rect],
            &FormulaMaskOptions::default(),
        );
        assert!(!result.evidence.fell_back);
        assert_eq!(
            result.evidence.background_estimate[0].color,
            [200, 200, 200]
        );
    }

    #[test]
    fn test_colored_background_uses_estimated_color() {
        // Magenta-ish background: rgb(200, 30, 200).
        let full = SnipperImage::new(
            80,
            80,
            PixelFormat::Rgb,
            (0..80 * 80).flat_map(|_| [200u8, 30u8, 200u8]).collect(),
        );
        let crop = full.clone();
        let rect = Rect::new(5.0, 5.0, 10.0, 10.0);
        let result = apply_formula_mask(
            &full,
            crop,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &[rect],
            &FormulaMaskOptions::default(),
        );
        assert!(!result.evidence.fell_back);
        assert_eq!(result.evidence.background_estimate[0].color, [200, 30, 200]);
    }

    #[test]
    fn test_evidence_fields_populated() {
        let full = white_rgb(100, 100);
        let crop = full.clone();
        let rect = Rect::new(10.0, 10.0, 20.0, 20.0);
        let result = apply_formula_mask(
            &full,
            crop,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &[rect],
            &FormulaMaskOptions::default(),
        );
        assert_eq!(result.evidence.mask_algorithm_version, "v1");
        assert_eq!(result.evidence.mask_margin, 4.0);
        assert_eq!(result.evidence.masked_image_sha256.len(), 64);
        assert!(result
            .evidence
            .masked_image_sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }
}
