use latexsnipper_ast::Rect;

use crate::image::SnipperImage;

/// Zero-copy view into a region of an image.
/// Avoids cloning pixel data during detection → recognition flow.
pub struct ImageView<'a> {
    image: &'a SnipperImage,
    rect: Rect,
}

impl<'a> ImageView<'a> {
    pub fn new(image: &'a SnipperImage, rect: Rect) -> Self {
        Self { image, rect }
    }

    pub fn image(&self) -> &'a SnipperImage {
        self.image
    }
    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn width(&self) -> u32 {
        self.rect.width.round() as u32
    }

    pub fn height(&self) -> u32 {
        self.rect.height.round() as u32
    }

    /// Extract the viewed region as a new SnipperImage (copies pixels).
    pub fn extract(&self) -> SnipperImage {
        let x = self.rect.x.round().max(0.0) as u32;
        let y = self.rect.y.round().max(0.0) as u32;
        let w = self.width();
        let h = self.height();
        let bpp = self.image.bytes_per_pixel();

        let mut pixels = Vec::with_capacity((w * h * bpp as u32) as usize);
        for row in 0..h {
            let src_offset = ((y + row) * self.image.width() + x) * bpp as u32;
            let src_end = src_offset + w * bpp as u32;
            let src_slice = &self.image.pixels()[src_offset as usize..src_end as usize];
            pixels.extend_from_slice(src_slice);
        }

        SnipperImage::new(w, h, self.image.format(), pixels)
    }
}
