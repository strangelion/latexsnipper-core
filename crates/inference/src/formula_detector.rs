use latexsnipper_ast::Rect;
use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::{InferenceSession, ModelHandle};
use latexsnipper_tensor::Tensor;

use crate::types::DetectionBox;

const TARGET_SIZE: u32 = 768;
const CONF_THRESHOLD: f32 = 0.25;
const IOU_THRESHOLD: f32 = 0.45;

/// Detect formula regions in an image using YOLOv8.
/// Inference only depends on Session trait, not RuntimeBackend.
pub fn detect_formulas(
    image: &SnipperImage,
    session: &dyn InferenceSession,
) -> Result<Vec<DetectionBox>> {
    let (letterboxed, scale, pad_x, pad_y) = latexsnipper_image::operations::letterbox(image, TARGET_SIZE);

    let mean = [0.0, 0.0, 0.0];
    let std = [1.0, 1.0, 1.0];
    let pixels = latexsnipper_image::operations::normalize(&letterboxed, &mean, &std);

    let input = Tensor::float32("images", vec![1, 3, TARGET_SIZE as usize, TARGET_SIZE as usize], pixels);

    let outputs = session.run(&[input])?;

    // 5. Decode output
    let output = outputs.first().ok_or_else(|| SnipperError::Inference("No output tensor".into()))?;
    let raw_data = output.as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output is not float32".into()))?;
    let shape = output.shape();

    // Output shape: [1, num_anchors, 6] or [1, 6, num_anchors]
    let boxes = decode_yolo_output(raw_data, shape, scale, pad_x, pad_y)?;

    // 6. NMS
    let nms_boxes = nms(boxes, IOU_THRESHOLD);

    Ok(nms_boxes)
}

fn decode_yolo_output(
    data: &[f32],
    shape: &[usize],
    scale: f32,
    pad_x: f32,
    pad_y: f32,
) -> Result<Vec<DetectionBox>> {
    let mut boxes = Vec::new();

    // Determine layout: [1, N, 6] or [1, 6, N]
    let (num_anchors, is_transposed) = if shape.len() == 3 {
        if shape[2] == 6 {
            (shape[1], true) // [1, 6, N]
        } else {
            (shape[2], false) // [1, N, 6]
        }
    } else {
        return Err(SnipperError::Inference(format!("Unexpected YOLO output shape: {:?}", shape)));
    };

    for i in 0..num_anchors {
        let (cx, cy, w, h, conf0, conf1) = if is_transposed {
            (
                data[i],                          // cx
                data[num_anchors + i],            // cy
                data[2 * num_anchors + i],        // w
                data[3 * num_anchors + i],        // h
                data[4 * num_anchors + i],        // class 0 score (embedding)
                data[5 * num_anchors + i],        // class 1 score (isolated)
            )
        } else {
            let base = i * 6;
            (data[base], data[base + 1], data[base + 2], data[base + 3], data[base + 4], data[base + 5])
        };

        let max_conf = conf0.max(conf1);
        if max_conf < CONF_THRESHOLD {
            continue;
        }

        let class_id = if conf1 > conf0 { 1 } else { 0 };
        let class_name = if class_id == 1 { "isolated" } else { "embedding" }.to_string();

        // Convert from center to corner coordinates
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
