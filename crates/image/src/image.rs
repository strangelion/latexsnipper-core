use crate::color::PixelFormat;

/// Platform-independent image representation.
/// Core only knows this type, never DynamicImage or Bitmap.
#[derive(Debug, Clone)]
pub struct SnipperImage {
    width: u32,
    height: u32,
    format: PixelFormat,
    pixels: Vec<u8>,
}

impl SnipperImage {
    pub fn new(width: u32, height: u32, format: PixelFormat, pixels: Vec<u8>) -> Self {
        let bpp = match format {
            PixelFormat::Gray => 1,
            PixelFormat::Rgb | PixelFormat::Bgr => 3,
            PixelFormat::Rgba | PixelFormat::Bgra => 4,
        };
        let expected = width as usize * height as usize * bpp;
        if pixels.len() != expected {
            panic!(
                "Pixel buffer size mismatch: expected {} ({}x{}x{}), got {}",
                expected, width, height, bpp, pixels.len()
            );
        }
        Self { width, height, format, pixels }
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn format(&self) -> PixelFormat { self.format }
    pub fn pixels(&self) -> &[u8] { &self.pixels }
    pub fn pixels_mut(&mut self) -> &mut Vec<u8> { &mut self.pixels }
    pub fn into_pixels(self) -> Vec<u8> { self.pixels }

    pub fn bytes_per_pixel(&self) -> usize {
        match self.format {
            PixelFormat::Gray => 1,
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
            PixelFormat::Bgr => 3,
            PixelFormat::Bgra => 4,
        }
    }

    pub fn row_bytes(&self) -> usize {
        self.width as usize * self.bytes_per_pixel()
    }

    /// Get a pixel value at (x, y).
    pub fn get_pixel(&self, x: u32, y: u32) -> &[u8] {
        let offset = ((y * self.width + x) * self.bytes_per_pixel() as u32) as usize;
        &self.pixels[offset..offset + self.bytes_per_pixel()]
    }

    /// Create from raw pixel buffer ( caller must ensure correct size ).
    pub fn from_raw(width: u32, height: u32, format: PixelFormat, data: Vec<u8>) -> Self {
        Self { width, height, format, pixels: data }
    }
}
