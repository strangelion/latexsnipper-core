use serde::{Deserialize, Serialize};

/// Pixel format for images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    Gray,
    Rgb,
    Rgba,
    Bgr,
    Bgra,
}

impl PixelFormat {
    pub fn channels(&self) -> usize {
        match self {
            PixelFormat::Gray => 1,
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
            PixelFormat::Bgr => 3,
            PixelFormat::Bgra => 4,
        }
    }
}
