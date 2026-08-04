use async_trait::async_trait;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;

use crate::artifacts::CropRegion;
use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Crops detected regions from the image.
///
/// Reads detection results from context artifacts, crops each region,
/// and stores the cropped images in context artifacts for downstream nodes.
pub struct CropNode {
    name: String,
    min_size: u32,
}

impl CropNode {
    pub fn new(min_size: u32) -> Self {
        Self {
            name: "crop".into(),
            min_size,
        }
    }
}

impl Default for CropNode {
    fn default() -> Self {
        Self::new(4)
    }
}

#[async_trait]
impl PipelineNode for CropNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        let mut total_crops = 0;

        // Crop formula detections
        let formula_detections = ctx.artifacts.formula_detections.clone();
        let mut formula_crops = Vec::new();
        for det in &formula_detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if w >= self.min_size && h >= self.min_size {
                let cropped = operations::crop(
                    &image,
                    latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32),
                );
                formula_crops.push(CropRegion {
                    rect: det.rect,
                    image: cropped,
                });
                total_crops += 1;
            }
        }
        ctx.artifacts.formula_crops = formula_crops;

        // Crop text detections — use quad warp when available, fall back to rect crop.
        // Text crops overlapping formula regions are masked with the estimated
        // local background so the OCR model only sees pure text.
        let text_detections = ctx.artifacts.text_detections.clone();
        let formula_detections = ctx.artifacts.formula_detections.clone();
        let mut text_crops = Vec::new();
        let mut mask_evidence = Vec::new();
        for (det_idx, det) in text_detections.iter().enumerate() {
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if w >= self.min_size && h >= self.min_size {
                let cropped = if let Some(ref quad) = det.quad {
                    let (tw, th) = quad.warp_target_size();
                    let padding = (th as f32 * 0.1).max(2.0); // 10% height padding
                    operations::warp_quad_to_rect(&image, quad, tw.max(4), th.max(4), padding)
                } else {
                    let x = det.rect.x as u32;
                    let y = det.rect.y as u32;
                    operations::crop(
                        &image,
                        latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32),
                    )
                };

                // Background-aware formula mask: fill any formula region that
                // intersects this text crop before OCR.
                let intersecting: Vec<latexsnipper_ast::Rect> = formula_detections
                    .iter()
                    .filter(|fd| {
                        let overlap_x =
                            det.rect.right().min(fd.rect.right()) - det.rect.x.max(fd.rect.x);
                        let overlap_y =
                            det.rect.bottom().min(fd.rect.bottom()) - det.rect.y.max(fd.rect.y);
                        overlap_x > 0.0 && overlap_y > 0.0
                    })
                    .map(|fd| fd.rect)
                    .collect();
                let masked = crate::formula_mask::apply_formula_mask(
                    &image,
                    cropped,
                    det.rect,
                    &intersecting,
                    &crate::formula_mask::FormulaMaskOptions::default(),
                );
                // Record evidence for every text crop that intersects a
                // formula, including fallback runs: a fallback must be
                // observable, not silently dropped.
                if !intersecting.is_empty() {
                    mask_evidence.push(crate::artifacts::TextCropMaskEvidence {
                        text_detection_index: det_idx,
                        mask: masked.evidence,
                    });
                }
                text_crops.push(CropRegion {
                    rect: det.rect,
                    image: masked.image,
                });
                total_crops += 1;
            }
        }
        ctx.artifacts.text_crops = text_crops;
        ctx.artifacts.text_crop_mask_evidence = mask_evidence;

        // Crop handwriting detections
        let handwriting_detections = ctx.artifacts.handwriting_detections.clone();
        let mut handwriting_crops = Vec::new();
        for det in &handwriting_detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if w >= self.min_size && h >= self.min_size {
                let cropped = operations::crop(
                    &image,
                    latexsnipper_ast::Rect::new(x as f32, y as f32, w as f32, h as f32),
                );
                handwriting_crops.push(CropRegion {
                    rect: det.rect,
                    image: cropped,
                });
                total_crops += 1;
            }
        }
        ctx.artifacts.handwriting_crops = handwriting_crops;

        if total_crops > 0 {
            log::info!("CropNode: cropped {} regions", total_crops);
        }

        Ok(())
    }
}
