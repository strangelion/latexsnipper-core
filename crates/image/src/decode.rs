use std::io::{BufReader, Cursor};
use std::path::Path;

use crate::color::PixelFormat;
use crate::image::SnipperImage;
use latexsnipper_ast::ImportOptions;
use latexsnipper_foundation::{Result, SnipperError};

/// Image input source.
pub enum ImageSource<'a> {
    File(&'a Path),
    Memory(&'a [u8]),
}

/// Decode an image from a file path or memory buffer.
pub fn decode(source: ImageSource) -> Result<SnipperImage> {
    decode_with_options(source, &ImportOptions::default())
}

/// Decode an image while enforcing the shared document import safety limits.
pub fn decode_with_options(
    source: ImageSource<'_>,
    options: &ImportOptions,
) -> Result<SnipperImage> {
    match source {
        ImageSource::File(path) => {
            let metadata =
                std::fs::metadata(path).map_err(|error| SnipperError::Io(error.to_string()))?;
            if metadata.len() > options.max_input_size {
                return Err(SnipperError::LimitExceeded(format!(
                    "input is {} bytes; limit is {} bytes",
                    metadata.len(),
                    options.max_input_size
                )));
            }
            let file =
                std::fs::File::open(path).map_err(|error| SnipperError::Io(error.to_string()))?;
            let reader = image::ImageReader::new(BufReader::new(file))
                .with_guessed_format()
                .map_err(|error| SnipperError::Io(error.to_string()))?;
            decode_reader(reader, options)
        }
        ImageSource::Memory(bytes) => {
            if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > options.max_input_size {
                return Err(SnipperError::LimitExceeded(format!(
                    "input is {} bytes; limit is {} bytes",
                    bytes.len(),
                    options.max_input_size
                )));
            }
            let reader = image::ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|error| SnipperError::Io(error.to_string()))?;
            decode_reader(reader, options)
        }
    }
}

fn decode_reader<R>(
    mut reader: image::ImageReader<R>,
    options: &ImportOptions,
) -> Result<SnipperImage>
where
    R: std::io::BufRead + std::io::Seek,
{
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(u32::try_from(options.max_image_width).unwrap_or(u32::MAX));
    limits.max_image_height = Some(u32::try_from(options.max_image_height).unwrap_or(u32::MAX));
    limits.max_alloc = Some(options.max_decompressed_size);
    reader.limits(limits);
    let img = reader.decode().map_err(map_decode_error)?;
    let pixels = u64::from(img.width())
        .checked_mul(u64::from(img.height()))
        .ok_or_else(|| SnipperError::LimitExceeded("image pixel count overflow".to_string()))?;
    if pixels > options.max_image_pixels {
        return Err(SnipperError::LimitExceeded(format!(
            "image has {pixels} pixels; limit is {}",
            options.max_image_pixels
        )));
    }
    Ok(to_snipper_image(&img))
}

fn map_decode_error(error: image::ImageError) -> SnipperError {
    match error {
        image::ImageError::Limits(error) => SnipperError::LimitExceeded(error.to_string()),
        other => SnipperError::Image(other.to_string()),
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
