use crate::color::PixelFormat;
use crate::image::SnipperImage;
use latexsnipper_ast::Rect;

/// Resize image to target dimensions.
pub fn resize(image: &SnipperImage, target_w: u32, target_h: u32) -> SnipperImage {
    let src_w = image.width();
    let src_h = image.height();
    let bpp = image.bytes_per_pixel();
    let mut pixels = vec![0u8; (target_w * target_h * bpp as u32) as usize];

    for ty in 0..target_h {
        for tx in 0..target_w {
            let sx = (tx as f32 * src_w as f32 / target_w as f32) as u32;
            let sy = (ty as f32 * src_h as f32 / target_h as f32) as u32;
            let sx = sx.min(src_w - 1);
            let sy = sy.min(src_h - 1);

            let src_off = ((sy * src_w + sx) * bpp as u32) as usize;
            let dst_off = ((ty * target_w + tx) * bpp as u32) as usize;
            pixels[dst_off..dst_off + bpp].copy_from_slice(&image.pixels()[src_off..src_off + bpp]);
        }
    }

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

/// Crop a rectangular region from the image.
pub fn crop(image: &SnipperImage, rect: Rect) -> SnipperImage {
    let x = rect.x.round().max(0.0) as u32;
    let y = rect.y.round().max(0.0) as u32;
    let w = rect.width.round() as u32;
    let h = rect.height.round() as u32;
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
