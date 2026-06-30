use latexsnipper_ast::Rect;
use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::DetectionBox;

/// Detection parameters loaded from config.json.
#[derive(Debug, Clone)]
pub struct DetectionParams {
    pub target_size: u32,
    pub conf_threshold: f32,
    pub iou_threshold: f32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    /// Whether to apply sigmoid to class scores.
    /// true = raw logits from model, need sigmoid
    /// false = model already outputs probabilities
    pub apply_sigmoid: bool,
    /// Output tensor layout: "row_major" [N,6] or "col_major" [6,N]
    pub output_layout: String,
}

impl Default for DetectionParams {
    fn default() -> Self {
        Self {
            target_size: 768,
            conf_threshold: 0.25,
            iou_threshold: 0.45,
            mean: [0.0, 0.0, 0.0],
            std: [1.0, 1.0, 1.0],
            apply_sigmoid: true,
            output_layout: "row_major".into(),
        }
    }
}

impl DetectionParams {
    /// Build from ModelConfig (config.json).
    pub fn from_config(config: &latexsnipper_model::ModelConfig) -> Self {
        let mean = config.normalization_mean();
        let std = config.normalization_std();
        let (w, h) = config.resize_dimensions();

        let post = config.postprocessing.as_ref();
        let conf_threshold = post.and_then(|p| p.confidence_threshold).unwrap_or(0.25);
        let iou_threshold = post.and_then(|p| p.iou_threshold).unwrap_or(0.45);
        let apply_sigmoid = post.and_then(|p| p.apply_sigmoid).unwrap_or(true);
        let output_layout = post.and_then(|p| p.output_layout.clone())
            .unwrap_or_else(|| "row_major".into());

        let target_size = w.or(h).unwrap_or(768);

        Self {
            target_size,
            conf_threshold,
            iou_threshold,
            mean: [mean[0], mean[1], mean[2]],
            std: [std[0], std[1], std[2]],
            apply_sigmoid,
            output_layout,
        }
    }
}

/// Detect formula regions in an image using YOLOv8.
pub fn detect_formulas(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    params: &DetectionParams,
) -> Result<Vec<DetectionBox>> {
    let (letterboxed, scale, pad_x, pad_y) = latexsnipper_image::operations::letterbox(image, params.target_size);

    let pixels = latexsnipper_image::operations::normalize(&letterboxed, &params.mean, &params.std);

    let input = Tensor::float32("images", vec![1, 3, params.target_size as usize, params.target_size as usize], pixels);

    let outputs = session.run(&[input])?;

    let output = outputs.first().ok_or_else(|| SnipperError::Inference("No output tensor".into()))?;
    let raw_data = output.as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output is not float32".into()))?;
    let shape = output.shape();

    let boxes = decode_yolo_output(raw_data, shape, scale, pad_x, pad_y, params)?;

    // Sort by confidence for better NMS
    let mut sorted_boxes = boxes;
    sorted_boxes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    
    // Apply NMS with stricter threshold
    let nms_boxes = nms(sorted_boxes, 0.5);
    
    // Limit to reasonable number of detections
    let mut final_boxes = nms_boxes;
    final_boxes.truncate(50);

    Ok(final_boxes)
}

fn decode_yolo_output(
    data: &[f32],
    shape: &[usize],
    scale: f32,
    pad_x: f32,
    pad_y: f32,
    params: &DetectionParams,
) -> Result<Vec<DetectionBox>> {
    let mut boxes = Vec::new();

    // Determine number of anchors from shape
    let num_anchors = if shape.len() == 3 { shape[1].max(shape[2]) }
        else if shape.len() == 2 { shape[0].max(shape[1]) }
        else { return Err(SnipperError::Inference(format!("Unexpected YOLO shape: {:?}", shape))); };

    // Handle layout: col_major needs transpose to row_major
    let is_col_major = params.output_layout == "col_major";
    let num_anchors_actual = if is_col_major {
        // [6, N] layout: shape[smaller] is anchor count
        if shape.len() == 3 { shape[1].min(shape[2]) } else { shape[0].min(shape[1]) }
    } else {
        num_anchors
    };

    for i in 0..num_anchors_actual {
        let (cx, cy, w, h, raw_conf0, raw_conf1) = if is_col_major {
            let n = num_anchors_actual;
            (data[i], data[n + i], data[2*n + i], data[3*n + i], data[4*n + i], data[5*n + i])
        } else {
            let base = i * 6;
            if base + 5 >= data.len() { break; }
            (data[base], data[base + 1], data[base + 2], data[base + 3], data[base + 4], data[base + 5])
        };

        let conf0 = if params.apply_sigmoid { sigmoid(raw_conf0) } else { raw_conf0 };
        let conf1 = if params.apply_sigmoid { sigmoid(raw_conf1) } else { raw_conf1 };

        let max_conf = conf0.max(conf1);
        if max_conf < params.conf_threshold {
            continue;
        }

        let class_id = if conf1 > conf0 { 1 } else { 0 };
        let class_name = if class_id == 1 { "isolated" } else { "embedding" }.to_string();

        let x1 = (cx - w / 2.0 - pad_x) / scale;
        let y1 = (cy - h / 2.0 - pad_y) / scale;
        let bw = w / scale;
        let bh = h / scale;

        boxes.push(DetectionBox {
            rect: Rect::new(x1.max(0.0), y1.max(0.0), bw, bh),
            confidence: max_conf,
            class_id,
            class_name,
        });
    }

    Ok(boxes)
}

fn nms(mut boxes: Vec<DetectionBox>, iou_threshold: f32) -> Vec<DetectionBox> {
    boxes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    let mut keep = Vec::new();
    let mut suppressed = vec![false; boxes.len()];

    for i in 0..boxes.len() {
        if suppressed[i] {
            continue;
        }
        keep.push(boxes[i].clone());

        for j in (i + 1)..boxes.len() {
            if suppressed[j] {
                continue;
            }
            if boxes[i].rect.iou(&boxes[j].rect) > iou_threshold {
                suppressed[j] = true;
            }
        }
    }

    keep
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Group nearby formula detections into complete expressions.
/// This matches LaTeXSnipper's behavior of merging adjacent formula boxes.
pub fn group_formula_detections(boxes: &mut Vec<DetectionBox>) {
    if boxes.is_empty() {
        return;
    }

    // Sort by Y position, then X position
    boxes.sort_by(|a, b| {
        a.rect.y.partial_cmp(&b.rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.rect.x.partial_cmp(&b.rect.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut merged = Vec::new();
    let mut used = vec![false; boxes.len()];

    for i in 0..boxes.len() {
        if used[i] {
            continue;
        }

        let mut current = boxes[i].clone();
        used[i] = true;

        // Try to merge with nearby boxes
        for j in (i + 1)..boxes.len() {
            if used[j] {
                continue;
            }

            let other = &boxes[j];

            // Check if on same line (Y centers within 1.5x average height)
            let avg_height = (current.rect.height + other.rect.height) / 2.0;
            let y_center_diff = (current.rect.center_y() - other.rect.center_y()).abs();

            if y_center_diff > avg_height * 1.5 {
                continue; // Too far vertically
            }

            // Check X proximity (gap < 50% of average width)
            let x_gap = if other.rect.x > current.rect.right() {
                other.rect.x - current.rect.right()
            } else if current.rect.x > other.rect.right() {
                current.rect.x - other.rect.right()
            } else {
                0.0 // Overlapping
            };

            let avg_width = (current.rect.width + other.rect.width) / 2.0;
            if x_gap < avg_width * 0.5 {
                // Merge boxes
                let x1 = current.rect.x.min(other.rect.x);
                let y1 = current.rect.y.min(other.rect.y);
                let x2 = current.rect.right().max(other.rect.right());
                let y2 = current.rect.bottom().max(other.rect.bottom());
                current.rect = Rect::new(x1, y1, x2 - x1, y2 - y1);
                current.confidence = current.confidence.max(other.confidence);
                used[j] = true;
            }
        }

        merged.push(current);
    }

    *boxes = merged;
}

/// Filter formula detections by size and confidence.
pub fn filter_formula_detections(boxes: &mut Vec<DetectionBox>, min_area: f32, min_confidence: f32) {
    boxes.retain(|b| {
        let area = b.rect.width * b.rect.height;
        area >= min_area && b.confidence >= min_confidence
    });
}
