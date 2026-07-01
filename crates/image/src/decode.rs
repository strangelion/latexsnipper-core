use std::path::Path;

use crate::color::PixelFormat;
use crate::image::SnipperImage;
use latexsnipper_foundation::{Result, SnipperError};

/// Image input source.
pub enum ImageSource<'a> {
    File(&'a Path),
    Memory(&'a [u8]),
}

/// Decode an image from a file path or memory buffer.
pub fn decode(source: ImageSource) -> Result<SnipperImage> {
    match source {
        ImageSource::File(path) => {
            let img = image::open(path).map_err(|e| SnipperError::Image(e.to_string()))?;
            Ok(to_snipper_image(&img))
        }
        ImageSource::Memory(bytes) => {
            let img =
                image::load_from_memory(bytes).map_err(|e| SnipperError::Image(e.to_string()))?;
            Ok(to_snipper_image(&img))
        }
    }
}

fn to_snipper_image(img: &image::DynamicImage) -> SnipperImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let pixels = rgba.into_raw();
    SnipperImage::new(w, h, PixelFormat::Rgba, pixels)
}

/// Encode image to PNG bytes.
pub fn encode_png(image: &SnipperImage) -> Result<Vec<u8>> {
    let rgba = image::RgbaImage::from_raw(image.width(), image.height(), image.pixels().to_vec())
        .ok_or_else(|| SnipperError::Image("Invalid image dimensions".into()))?;

    let mut buf = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| SnipperError::Image(e.to_string()))?;
    Ok(buf.into_inner())
}
