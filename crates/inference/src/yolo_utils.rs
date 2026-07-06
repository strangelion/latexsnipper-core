use crate::types::DetectionBox;
use latexsnipper_ast::Rect;
use latexsnipper_foundation::{Result, SnipperError};

/// Shared YOLO detection parameters.
#[derive(Debug, Clone)]
pub struct YoloParams {
    pub target_size: u32,
    pub conf_threshold: f32,
    pub iou_threshold: f32,
    pub apply_sigmoid: bool,
    pub output_layout: String,
}

/// Decode a YOLO output tensor into detection boxes.
///
/// Supports both row-major (batch, anchors, channels) and column-major
/// (channels, anchors) output layouts.
pub fn decode_yolo_output(
    data: &[f32],
    shape: &[usize],
    scale: f32,
    pad_x: f32,
    pad_y: f32,
    params: &YoloParams,
) -> Result<Vec<DetectionBox>> {
    let mut boxes = Vec::new();

    // Determine number of anchors from shape
    let num_anchors = if shape.len() == 3 {
        shape[1].max(shape[2])
    } else if shape.len() == 2 {
        shape[0].max(shape[1])
    } else {
        return Err(SnipperError::Inference(format!(
            "Unexpected YOLO shape: {:?}",
            shape
        )));
    };

    // Handle layout: col_major needs transpose to row_major
    let is_col_major = params.output_layout == "col_major";
    let num_anchors_actual = if is_col_major {
        // [B, 6, N] or [6, N] layout: last dim is anchor count (stride)
        shape[shape.len() - 1]
    } else {
        num_anchors
    };

    for i in 0..num_anchors_actual {
        let (cx, cy, w, h, raw_conf0, raw_conf1) = if is_col_major {
            let n = num_anchors_actual;
            (
                data[i],
                data[n + i],
                data[2 * n + i],
                data[3 * n + i],
                data[4 * n + i],
                data[5 * n + i],
            )
        } else {
            let base = i * 6;
            if base + 5 >= data.len() {
                break;
            }
            (
                data[base],
                data[base + 1],
                data[base + 2],
                data[base + 3],
                data[base + 4],
                data[base + 5],
            )
        };

        let conf0 = if params.apply_sigmoid {
            sigmoid(raw_conf0)
        } else {
            raw_conf0
        };
        let conf1 = if params.apply_sigmoid {
            sigmoid(raw_conf1)
        } else {
            raw_conf1
        };

        let max_conf = conf0.max(conf1);
        if max_conf < params.conf_threshold {
            continue;
        }

        let class_id = 0;
        let class_name = String::new();

        let x1 = (cx - w / 2.0 - pad_x) / scale;
        let y1 = (cy - h / 2.0 - pad_y) / scale;
        let bw = w / scale;
        let bh = h / scale;

        boxes.push(DetectionBox::rect(
            Rect::new(x1.max(0.0), y1.max(0.0), bw, bh),
            max_conf,
            class_id,
            class_name,
        ));
    }

    Ok(boxes)
}

/// Non-maximum suppression: remove overlapping boxes, keeping the highest
/// confidence one.
pub fn nms(mut boxes: Vec<DetectionBox>, iou_threshold: f32) -> Vec<DetectionBox> {
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

/// Sigmoid activation function.
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
