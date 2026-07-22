use crate::color::PixelFormat;
use crate::image::SnipperImage;
use latexsnipper_ast::{Quad, Rect};

/// Resize image to target dimensions.
pub fn resize(image: &SnipperImage, target_w: u32, target_h: u32) -> SnipperImage {
    let src_w = image.width();
    let src_h = image.height();
    let bpp = image.bytes_per_pixel();
    let mut pixels = vec![0u8; (target_w * target_h * bpp as u32) as usize];

    for ty in 0..target_h {
        for tx in 0..target_w {
            let sx = (tx as f32 * src_w as f32 / target_w as f32 + 0.5) as u32;
            let sy = (ty as f32 * src_h as f32 / target_h as f32 + 0.5) as u32;
            let sx = sx.min(src_w - 1);
            let sy = sy.min(src_h - 1);

            let src_off = ((sy * src_w + sx) * bpp as u32) as usize;
            let dst_off = ((ty * target_w + tx) * bpp as u32) as usize;
            pixels[dst_off..dst_off + bpp].copy_from_slice(&image.pixels()[src_off..src_off + bpp]);
        }
    }

    SnipperImage::new(target_w, target_h, image.format(), pixels)
}

/// Resize an image with bilinear interpolation.
pub fn resize_bilinear(image: &SnipperImage, target_w: u32, target_h: u32) -> SnipperImage {
    resize_filtered(
        image,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    )
}

/// Resize an image with bicubic interpolation.
pub fn resize_bicubic(image: &SnipperImage, target_w: u32, target_h: u32) -> SnipperImage {
    resize_filtered(
        image,
        target_w,
        target_h,
        image::imageops::FilterType::CatmullRom,
    )
}

fn resize_filtered(
    image: &SnipperImage,
    target_w: u32,
    target_h: u32,
    filter: image::imageops::FilterType,
) -> SnipperImage {
    assert!(
        target_w > 0 && target_h > 0,
        "resize target must be non-zero"
    );
    let pixels = match image.bytes_per_pixel() {
        1 => {
            let source =
                image::GrayImage::from_raw(image.width(), image.height(), image.pixels().to_vec())
                    .expect("SnipperImage gray buffer length is validated at construction");
            image::imageops::resize(&source, target_w, target_h, filter).into_raw()
        }
        3 => {
            let source =
                image::RgbImage::from_raw(image.width(), image.height(), image.pixels().to_vec())
                    .expect("SnipperImage RGB buffer length is validated at construction");
            image::imageops::resize(&source, target_w, target_h, filter).into_raw()
        }
        4 => {
            let source =
                image::RgbaImage::from_raw(image.width(), image.height(), image.pixels().to_vec())
                    .expect("SnipperImage RGBA buffer length is validated at construction");
            image::imageops::resize(&source, target_w, target_h, filter).into_raw()
        }
        channels => unreachable!("unsupported pixel channel count {channels}"),
    };
    SnipperImage::new(target_w, target_h, image.format(), pixels)
}

/// Resize to fit within max_side, preserving aspect ratio.
pub fn resize_to_fit(image: &SnipperImage, max_side: u32) -> SnipperImage {
    let w = image.width();
    let h = image.height();
    if w <= max_side && h <= max_side {
        return image.clone();
    }
    let scale = max_side as f32 / w.max(h) as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;
    resize(image, new_w, new_h)
}

/// Letterbox resize for YOLO: resize to (target, target) with gray padding.
pub fn letterbox(image: &SnipperImage, target: u32) -> (SnipperImage, f32, f32, f32) {
    let w = image.width() as f32;
    let h = image.height() as f32;
    let scale = (target as f32 / w.max(h)).min(1.0);
    let new_w = (w * scale).round() as u32;
    let new_h = (h * scale).round() as u32;
    let pad_x = ((target - new_w) / 2) as f32;
    let pad_y = ((target - new_h) / 2) as f32;

    let resized = resize(image, new_w, new_h);
    let bpp = resized.bytes_per_pixel();
    let mut pixels = vec![114u8; (target * target * bpp as u32) as usize];

    for y in 0..new_h {
        let src_off = (y * new_w * bpp as u32) as usize;
        let dst_off = ((y * target + pad_x as u32) * bpp as u32) as usize;
        let copy_len = new_w * bpp as u32;
        pixels[dst_off..dst_off + copy_len as usize]
            .copy_from_slice(&resized.pixels()[src_off..src_off + copy_len as usize]);
    }

    (
        SnipperImage::new(target, target, image.format(), pixels),
        scale,
        pad_x,
        pad_y,
    )
}

/// Normalize pixels to float range, return as f32 vector in CHW layout.
/// If image has more channels than mean/std, only uses first N channels.
pub fn normalize(image: &SnipperImage, mean: &[f32], std: &[f32]) -> Vec<f32> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let img_channels = image.format().channels();
    let out_channels = mean.len().min(img_channels);
    let mut output = vec![0.0f32; out_channels * h * w];

    for y in 0..h {
        for x in 0..w {
            let src_off = (y * w + x) * img_channels;
            for c in 0..out_channels {
                let pixel = image.pixels()[src_off + c] as f32 / 255.0;
                let normalized = (pixel - mean[c]) / std[c];
                output[c * h * w + y * w + x] = normalized;
            }
        }
    }
    output
}

/// Crop a rectangular region from the image, clamped to image bounds.
/// Returns a 1x1 black pixel if the rect is entirely outside the image.
pub fn crop(image: &SnipperImage, rect: Rect) -> SnipperImage {
    let img_w = image.width() as f32;
    let img_h = image.height() as f32;

    // Clamp to image bounds
    let x = rect.x.round().clamp(0.0, img_w) as u32;
    let y = rect.y.round().clamp(0.0, img_h) as u32;
    let w = (rect.width.round() as u32).min(image.width() - x);
    let h = (rect.height.round() as u32).min(image.height() - y);

    if w == 0 || h == 0 {
        // Return minimal valid image instead of panicking
        return SnipperImage::new(1, 1, image.format(), vec![0u8; image.bytes_per_pixel()]);
    }

    let bpp = image.bytes_per_pixel();

    let mut pixels = Vec::with_capacity((w * h * bpp as u32) as usize);
    for row in 0..h {
        let src_offset = ((y + row) * image.width() + x) * bpp as u32;
        let src_end = src_offset + w * bpp as u32;
        pixels.extend_from_slice(&image.pixels()[src_offset as usize..src_end as usize]);
    }

    SnipperImage::new(w, h, image.format(), pixels)
}

/// Convert BGR to RGB.
pub fn bgr_to_rgb(image: &SnipperImage) -> SnipperImage {
    if image.format() != PixelFormat::Bgr {
        return image.clone();
    }
    let mut pixels = image.pixels().to_vec();
    for chunk in pixels.chunks_exact_mut(3) {
        chunk.swap(0, 2);
    }
    SnipperImage::new(image.width(), image.height(), PixelFormat::Rgb, pixels)
}

/// Convert RGB to BGR.
pub fn rgb_to_bgr(image: &SnipperImage) -> SnipperImage {
    if image.format() != PixelFormat::Rgb {
        return image.clone();
    }
    let mut pixels = image.pixels().to_vec();
    for chunk in pixels.chunks_exact_mut(3) {
        chunk.swap(0, 2);
    }
    SnipperImage::new(image.width(), image.height(), PixelFormat::Bgr, pixels)
}

/// Convert RGBA to BGR (drop alpha channel).
/// The model expects BGR input (PP-OCR convention), but decoded images are RGBA.
pub fn rgba_to_bgr(image: &SnipperImage) -> SnipperImage {
    if image.format() != PixelFormat::Rgba {
        return image.clone();
    }
    let w = image.width();
    let h = image.height();
    let pixels = image.pixels();
    let mut bgr = Vec::with_capacity((w * h * 3) as usize);
    for chunk in pixels.chunks_exact(4) {
        // chunk = [R, G, B, A]
        bgr.push(chunk[2]); // B
        bgr.push(chunk[1]); // G
        bgr.push(chunk[0]); // R
    }
    SnipperImage::new(w, h, PixelFormat::Bgr, bgr)
}

/// Pad image to make dimensions divisible by stride.
pub fn pad_to_stride(image: &SnipperImage, stride: u32) -> SnipperImage {
    let w = image.width();
    let h = image.height();
    let new_w = w.div_ceil(stride) * stride;
    let new_h = h.div_ceil(stride) * stride;
    if new_w == w && new_h == h {
        return image.clone();
    }

    let bpp = image.bytes_per_pixel();
    let mut pixels = vec![0u8; (new_w * new_h * bpp as u32) as usize];

    for y in 0..h {
        let src_off = (y * w * bpp as u32) as usize;
        let dst_off = (y * new_w * bpp as u32) as usize;
        let copy_len = w * bpp as u32;
        pixels[dst_off..dst_off + copy_len as usize]
            .copy_from_slice(&image.pixels()[src_off..src_off + copy_len as usize]);
    }

    SnipperImage::new(new_w, new_h, image.format(), pixels)
}

/// Perspective warp: map a quadrilateral region to a rectangle (e.g. for OCR text line).
///
/// Uses a homography transformation with bilinear interpolation.
/// `quad` defines the source region (4 points in clockwise order).
/// `target_w` and `target_h` define the output rectangle dimensions.
/// A `padding` value is added to each side of the quad before warping.
pub fn warp_quad_to_rect(
    image: &SnipperImage,
    quad: &Quad,
    target_w: u32,
    target_h: u32,
    padding: f32,
) -> SnipperImage {
    let sorted = quad.sorted();

    // Apply padding by extending the quad edges outward
    let padded_quad = if padding > 0.0 {
        let p = padding;
        Quad::new(
            latexsnipper_ast::Point::new(sorted.p1.x - p, sorted.p1.y - p),
            latexsnipper_ast::Point::new(sorted.p2.x + p, sorted.p2.y - p),
            latexsnipper_ast::Point::new(sorted.p3.x + p, sorted.p3.y + p),
            latexsnipper_ast::Point::new(sorted.p4.x - p, sorted.p4.y + p),
        )
    } else {
        sorted
    };

    let img_w = image.width() as f64;
    let img_h = image.height() as f64;
    let bpp = image.bytes_per_pixel();
    let pixels = image.pixels();

    // Destination corners (rectangle)
    let dst = [
        (0.0f64, 0.0f64),
        (target_w as f64, 0.0),
        (target_w as f64, target_h as f64),
        (0.0, target_h as f64),
    ];

    // Source corners (quad)
    let src = [
        (padded_quad.p1.x as f64, padded_quad.p1.y as f64),
        (padded_quad.p2.x as f64, padded_quad.p2.y as f64),
        (padded_quad.p3.x as f64, padded_quad.p3.y as f64),
        (padded_quad.p4.x as f64, padded_quad.p4.y as f64),
    ];

    // Compute homography: dst = H * src
    // Solve for the 8 unknowns (h11...h32) of the homography matrix
    let homo = compute_homography(&src, &dst);

    // Allocate output
    let out_size = (target_w * target_h * bpp as u32) as usize;
    let mut out_pixels = vec![0u8; out_size];

    // Inverse warp: for each output pixel, find source coordinate
    let homo_inv = invert_homography(&homo);

    for ty in 0..target_h {
        for tx in 0..target_w {
            // Map destination pixel to source coordinate via inverse homography
            let sx_f = homo_inv[0] * tx as f64 + homo_inv[1] * ty as f64 + homo_inv[2];
            let sy_f = homo_inv[3] * tx as f64 + homo_inv[4] * ty as f64 + homo_inv[5];
            let sz = homo_inv[6] * tx as f64 + homo_inv[7] * ty as f64 + homo_inv[8];

            let sx = (sx_f / sz) as f32;
            let sy = (sy_f / sz) as f32;

            // Bilinear interpolation
            let dst_off = ((ty * target_w + tx) * bpp as u32) as usize;

            if sx >= 0.0 && sx < img_w as f32 - 1.0 && sy >= 0.0 && sy < img_h as f32 - 1.0 {
                let x0 = sx as u32;
                let y0 = sy as u32;
                let img_w_u = img_w as u32;
                let img_h_u = img_h as u32;
                let x1 = (x0 + 1).min(img_w_u - 1);
                let y1 = (y0 + 1).min(img_h_u - 1);
                let fx = sx - x0 as f32;
                let fy = sy - y0 as f32;

                for c in 0..bpp {
                    let p00 = pixels[((y0 * img_w_u + x0) * bpp as u32 + c as u32) as usize];
                    let p10 = pixels[((y0 * img_w_u + x1) * bpp as u32 + c as u32) as usize];
                    let p01 = pixels[((y1 * img_w_u + x0) * bpp as u32 + c as u32) as usize];
                    let p11 = pixels[((y1 * img_w_u + x1) * bpp as u32 + c as u32) as usize];

                    let top = p00 as f32 * (1.0 - fx) + p10 as f32 * fx;
                    let bottom = p01 as f32 * (1.0 - fx) + p11 as f32 * fx;
                    out_pixels[dst_off + c] = (top * (1.0 - fy) + bottom * fy).round() as u8;
                }
            }
        }
    }

    SnipperImage::new(target_w, target_h, image.format(), out_pixels)
}

/// Compute the 3x3 homography matrix mapping src → dst.
/// Returns the 9 elements in row-major order: [h11, h12, h13, h21, h22, h23, h31, h32, h33].
#[allow(clippy::needless_range_loop)]
fn compute_homography(src: &[(f64, f64); 4], dst: &[(f64, f64); 4]) -> [f64; 9] {
    // DLT algorithm: solve Ah = 0 via Gaussian elimination on an 8x8 system
    // Each point pair contributes 2 rows to the 8x9 matrix (8 unknowns, augmented)
    let mut a = [[0.0f64; 9]; 8];

    for i in 0..4 {
        let (sx, sy) = src[i];
        let (dx, dy) = dst[i];

        // First row: [ -sx, -sy, -1, 0, 0, 0, dx*sx, dx*sy, dx ]
        a[i * 2] = [-sx, -sy, -1.0, 0.0, 0.0, 0.0, dx * sx, dx * sy, dx];

        // Second row: [ 0, 0, 0, -sx, -sy, -1, dy*sx, dy*sy, dy ]
        a[i * 2 + 1] = [0.0, 0.0, 0.0, -sx, -sy, -1.0, dy * sx, dy * sy, dy];
    }

    // Gaussian elimination with partial pivoting
    for col in 0..8 {
        // Find pivot
        let mut max_val = a[col][col].abs();
        let mut max_row = col;
        for row in (col + 1)..8 {
            let val = a[row][col].abs();
            if val > max_val {
                max_val = val;
                max_row = row;
            }
        }

        if max_val < 1e-12 {
            continue;
        }

        // Swap rows
        a.swap(col, max_row);

        // Eliminate below
        for row in (col + 1)..8 {
            let factor = a[row][col] / a[col][col];
            for k in col..9 {
                a[row][k] -= factor * a[col][k];
            }
        }
    }

    // Back substitution
    let mut h = [0.0f64; 9];
    h[8] = 1.0; // h33 = 1 (scale)

    for i in (0..8).rev() {
        let mut sum = a[i][8];
        for j in (i + 1)..8 {
            sum -= a[i][j] * h[j];
        }
        if a[i][i].abs() > 1e-12 {
            h[i] = sum / a[i][i];
        }
    }

    h
}

/// Compute the inverse of a 3x3 homography matrix.
fn invert_homography(h: &[f64; 9]) -> [f64; 9] {
    let det = h[0] * (h[4] * h[8] - h[5] * h[7]) - h[1] * (h[3] * h[8] - h[5] * h[6])
        + h[2] * (h[3] * h[7] - h[4] * h[6]);

    if det.abs() < 1e-15 {
        return *h; // Fallback: return original (unlikely to happen with valid quads)
    }

    let inv_det = 1.0 / det;

    [
        (h[4] * h[8] - h[5] * h[7]) * inv_det,
        (h[2] * h[7] - h[1] * h[8]) * inv_det,
        (h[1] * h[5] - h[2] * h[4]) * inv_det,
        (h[5] * h[6] - h[3] * h[8]) * inv_det,
        (h[0] * h[8] - h[2] * h[6]) * inv_det,
        (h[2] * h[3] - h[0] * h[5]) * inv_det,
        (h[3] * h[7] - h[4] * h[6]) * inv_det,
        (h[1] * h[6] - h[0] * h[7]) * inv_det,
        (h[0] * h[4] - h[1] * h[3]) * inv_det,
    ]
}
