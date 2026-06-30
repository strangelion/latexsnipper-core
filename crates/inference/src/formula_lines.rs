/// Formula line splitting and grouping.
/// Ported from LaTeXSnipper's formula_lines.py.
use latexsnipper_image::SnipperImage;

/// A cropped region of a formula line.
#[derive(Debug, Clone)]
pub struct FormulaLineCrop {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A group of formula line crops that should be recognized together.
#[derive(Debug)]
pub struct FormulaLineGroup {
    pub crops: Vec<FormulaLineCrop>,
}

/// Split a formula image into line groups for recognition.
/// Each group contains one or more line crops.
pub fn split_formula_line_groups(image: &SnipperImage) -> Vec<FormulaLineGroup> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    
    if h < 24 || w < 12 {
        return vec![];
    }

    let gray = to_grayscale(image);
    let mask = ink_mask(&gray, w, h);
    let bands = row_bands(&mask, w, h);
    
    if bands.len() < 2 {
        // Single line or no split needed
        let crop = FormulaLineCrop {
            pixels: image.pixels().to_vec(),
            width: w as u32,
            height: h as u32,
        };
        return vec![FormulaLineGroup { crops: vec![crop] }];
    }
    
    let mut groups = Vec::new();
    for (top, bottom) in &bands {
        if let Some(crop) = crop_line(image, *top, *bottom) {
            groups.push(FormulaLineGroup { crops: vec![crop] });
        }
    }
    
    groups
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
    let threshold = threshold.max(80.0).min(245.0);
    
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
}
