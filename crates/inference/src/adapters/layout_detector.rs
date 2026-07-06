//! Layout analysis model adapter (PicoDet-based, e.g. PP-DocLayout CDLA).
//!
//! Accepts a full-page image and outputs detected layout regions:
//! text, title, figure, table, equation, header, footer, etc.

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    AccelerationMode, InferenceContext, InferenceSession, ModelDescriptor, ModelExecutor, ModelId,
    ModelInput, ModelOutput, ModelPackage, ModelTask, RuntimeBackend, TensorDtype, TensorSpec,
};
use std::path::PathBuf;
use std::sync::Arc;

/// CDLA layout labels used by the PP-Layout CDLA model.
pub const CDLA_LABELS: &[&str] = &[
    "text", "title", "figure", "figure_caption", "table", "table_caption", "header", "footer",
    "reference", "equation",
];

/// Layout detection model package (PicoDet-based).
pub struct LayoutDetectorPackage {
    descriptor: ModelDescriptor,
    model_path: Option<PathBuf>,
}

impl LayoutDetectorPackage {
    pub fn from_config(config: &ModelConfig, model_id: ModelId) -> Self {
        let input_shape = config
            .input
            .as_ref()
            .map(|i| i.shape.iter().map(|s| *s as usize).collect())
            .unwrap_or_else(|| vec![1, 3, 800, 608]);

        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::LayoutAnalysis,
            version: config
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            input_spec: TensorSpec {
                name: "image".into(),
                shape: input_shape,
                dtype: TensorDtype::Float32,
            },
            output_spec: vec![
                TensorSpec {
                    name: "transpose_0.tmp_0".into(),
                    shape: vec![1, 7600, 10],
                    dtype: TensorDtype::Float32,
                },
                TensorSpec {
                    name: "transpose_2.tmp_0".into(),
                    shape: vec![1, 1900, 10],
                    dtype: TensorDtype::Float32,
                },
                TensorSpec {
                    name: "transpose_4.tmp_0".into(),
                    shape: vec![1, 475, 10],
                    dtype: TensorDtype::Float32,
                },
                TensorSpec {
                    name: "transpose_6.tmp_0".into(),
                    shape: vec![1, 130, 10],
                    dtype: TensorDtype::Float32,
                },
            ],
            artifact_paths: vec![],
        };

        Self {
            descriptor,
            model_path: None,
        }
    }

    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl ModelPackage for LayoutDetectorPackage {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>> {
        let model_path = self.model_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No model path configured for LayoutDetector".into())
        })?;

        Ok(Box::new(LayoutDetectorExecutor {
            descriptor: self.descriptor.clone(),
            runtime,
            model_path: model_path.clone(),
            session: None,
        }))
    }
}

struct LayoutDetectorExecutor {
    descriptor: ModelDescriptor,
    runtime: Arc<dyn RuntimeBackend>,
    model_path: PathBuf,
    session: Option<Arc<Box<dyn InferenceSession>>>,
}

impl LayoutDetectorExecutor {
    fn ensure_loaded(&mut self) -> Result<&Arc<Box<dyn InferenceSession>>> {
        if self.session.is_some() {
            return Ok(self.session.as_ref().unwrap());
        }

        let handle =
            latexsnipper_runtime::ModelHandle::with_path(
                self.descriptor.id.composite_key(),
                self.model_path.clone(),
            );
        let session = self
            .runtime
            .create_session(&handle, AccelerationMode::Cpu)?;
        self.session = Some(Arc::new(session));
        Ok(self.session.as_ref().unwrap())
    }
}

impl ModelExecutor for LayoutDetectorExecutor {
    fn run(&mut self, input: ModelInput, _ctx: &mut InferenceContext) -> Result<ModelOutput> {
        let session = self.ensure_loaded()?.clone();

        // Reconstruct image from input bytes, resize to model's expected size
        let shape = &input.shape;
        if shape.len() != 3 {
            return Err(SnipperError::Inference(format!(
                "Expected 3D shape [H, W, 3], got {:?}",
                shape
            )));
        }
        let orig_h = shape[0] as u32;
        let orig_w = shape[1] as u32;
        let pixels: Vec<u8> = input.data.to_vec();

        let image = latexsnipper_image::SnipperImage::new(
            orig_w,
            orig_h,
            latexsnipper_image::color::PixelFormat::Rgb,
            pixels,
        );

        // Resize to model input size (800x608)
        let target_h = 800u32;
        let target_w = 608u32;
        let resized = latexsnipper_image::operations::resize(&image, target_w, target_h);

        // Normalize using ImageNet stats
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        let normalized =
            latexsnipper_image::operations::normalize(&resized, &mean, &std);

        let input_tensor = latexsnipper_tensor::Tensor::float32(
            "image",
            vec![1, 3, target_h as usize, target_w as usize],
            normalized,
        );

        let outputs = session.as_ref().as_ref().run(&[input_tensor])?;

        // PicoDet multi-head decode: collect detections from all FPN levels
        let score_threshold = 0.3;
        let nms_threshold = 0.5;

        // FPN levels with their strides
        let fpn_strides = [16.0f32, 32.0, 64.0, 128.0]; // 4 FPN levels → strides 8,16,32,64 → mapped to output channels
        let head_shapes = [(7600usize, 10usize), (1900, 10), (475, 10), (130, 10)];

        // For each FPN level: decode (cx, cy, w, h) from first 4 channels, scores from cls channels
        // The 10-channel output is: [x, y, w, h, cls0, ..., cls9] in sigmoid-normalized format
        // Map from feature map grid to image coordinates using stride

        let mut candidates: Vec<(latexsnipper_ast::Rect, f32, usize)> = Vec::new();

        for (level_idx, &(num_anchors, _)) in head_shapes.iter().enumerate() {
            let stride = fpn_strides[level_idx];
            let fmap_h = (target_h as f32 / stride).ceil() as usize;
            let fmap_w = (target_w as f32 / stride).ceil() as usize;

            // Calculate actual grid points
            let grid_size = fmap_h * fmap_w;
            if grid_size != num_anchors {
                // The head shapes don't directly correspond to stride-based grids.
                // For PicoDet, use the raw output shape.
                continue;
            }
        }

        // Simplified: process all level outputs, decode each as [cx, cy, w, h, scores...]
        for (level_idx, &(num_anchors, _num_channels)) in head_shapes.iter().enumerate() {
            if outputs.len() <= level_idx {
                continue;
            }

            // Check if this output has the expected shape
            let out_shape = outputs[level_idx].shape();
            if out_shape.len() < 3 {
                continue;
            }

            let actual_anchors = out_shape[1];
            let actual_channels = out_shape[2];
            if actual_channels < 6 {
                continue; // Need at least [cx, cy, w, h, cls] 
            }

            let data = match outputs[level_idx].as_f32_slice() {
                Some(d) => d,
                None => continue,
            };

            let stride = fpn_strides[level_idx];
            // Feature map dimensions
            let fmap_h = (target_h as f32 / stride).ceil() as usize;
            let fmap_w = (target_w as f32 / stride).ceil() as usize;
            let expected_grid = fmap_h * fmap_w;

            if actual_anchors != expected_grid {
                // The head shape doesn't match stride-based grid; 
                // use actual_anchors directly
            }

            // Decode each anchor point
            for anchor_idx in 0..actual_anchors.min(num_anchors) {
                let base = anchor_idx * actual_channels;

                // For PicoDet: first 4 values are [x, y, w, h] in grid-relative coords
                let x = data[base];
                let y = data[base + 1];
                let w_val = data[base + 2];
                let h_val = data[base + 3];

                // Find best class and score (channels 4+)
                let mut best_cls = 0usize;
                let mut best_score = f32::NEG_INFINITY;
                let num_cls = actual_channels.saturating_sub(4).min(10);
                for c in 0..num_cls {
                    let s = data[base + 4 + c];
                    if s > best_score {
                        best_score = s;
                        best_cls = c;
                    }
                }

                if best_score < score_threshold {
                    continue;
                }

                // Decode from grid to image coordinates
                let grid_x = (anchor_idx % fmap_w) as f32;
                let grid_y = (anchor_idx / fmap_w) as f32;

                // PicoDet: x,y are offset from grid center, w,h are relative to stride
                let cx = (grid_x + x.sigmoid()) * stride;
                let cy = (grid_y + y.sigmoid()) * stride;
                let bw = w_val.exp() * stride;
                let bh = h_val.exp() * stride;

                // Clamp to model image bounds
                let x1 = (cx - bw / 2.0).max(0.0).min(target_w as f32);
                let y1 = (cy - bh / 2.0).max(0.0).min(target_h as f32);
                let x2 = (cx + bw / 2.0).max(0.0).min(target_w as f32);
                let y2 = (cy + bh / 2.0).max(0.0).min(target_h as f32);

                // Scale back to original image coordinates
                let scale_x = orig_w as f32 / target_w as f32;
                let scale_y = orig_h as f32 / target_h as f32;

                let orig_rect = latexsnipper_ast::Rect::new(
                    x1 * scale_x,
                    y1 * scale_y,
                    (x2 - x1) * scale_x,
                    (y2 - y1) * scale_y,
                );

                candidates.push((orig_rect, best_score, best_cls));
            }
        }

        // Apply NMS
        struct Detection {
            rect: latexsnipper_ast::Rect,
            score: f32,
            class: usize,
        }

        let mut dets: Vec<Detection> = candidates
            .into_iter()
            .map(|(rect, score, class)| Detection { rect, score, class })
            .collect();

        dets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let mut keep: Vec<usize> = Vec::new();
        for i in 0..dets.len() {
            let mut suppressed = false;
            for &j in &keep {
                if iou(&dets[i].rect, &dets[j].rect) > nms_threshold {
                    suppressed = true;
                    break;
                }
            }
            if !suppressed {
                keep.push(i);
            }
        }

        // Build output
        let results: Vec<latexsnipper_runtime::LayoutResult> = keep
            .into_iter()
            .map(|i| {
                let d = &dets[i];
                let label = CDLA_LABELS.get(d.class).unwrap_or(&"unknown").to_string();
                latexsnipper_runtime::LayoutResult {
                    region_type: label,
                    x: d.rect.x,
                    y: d.rect.y,
                    width: d.rect.width,
                    height: d.rect.height,
                    confidence: d.score,
                }
            })
            .collect();

        Ok(ModelOutput::Layout(results))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}

fn iou(a: &latexsnipper_ast::Rect, b: &latexsnipper_ast::Rect) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = a.right().min(b.right());
    let y2 = a.bottom().min(b.bottom());
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a.width * a.height + b.width * b.height - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Simple sigmoid helper.
trait Sigmoid {
    fn sigmoid(&self) -> Self;
}

impl Sigmoid for f32 {
    fn sigmoid(&self) -> f32 {
        1.0 / (1.0 + (-self).exp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigmoid() {
        assert!((0.0f32.sigmoid() - 0.5).abs() < 1e-6);
        assert!(10.0f32.sigmoid() > 0.999);
        assert!((-10.0f32.sigmoid()) < 0.001);
    }

    #[test]
    fn test_iou() {
        let a = latexsnipper_ast::Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = latexsnipper_ast::Rect::new(5.0, 5.0, 10.0, 10.0);
        let i = iou(&a, &b);
        assert!(i > 0.0 && i < 1.0);
    }

    #[test]
    fn test_labels() {
        assert_eq!(CDLA_LABELS.len(), 10);
        assert_eq!(CDLA_LABELS[0], "text");
        assert_eq!(CDLA_LABELS[9], "equation");
    }
}
