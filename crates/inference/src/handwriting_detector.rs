use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::DetectionBox;
use crate::yolo_utils::{decode_yolo_output, nms};

/// Handwriting detection parameters.
#[derive(Debug, Clone)]
pub struct HandwritingDetParams {
    pub target_size: u32,
    pub conf_threshold: f32,
    pub iou_threshold: f32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub apply_sigmoid: bool,
    pub output_layout: String,
}

impl Default for HandwritingDetParams {
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

impl HandwritingDetParams {
    /// Build from ModelConfig (config.json).
    pub fn from_config(config: &latexsnipper_model::ModelConfig) -> Self {
        let mean = config.normalization_mean();
        let std = config.normalization_std();
        let (w, h) = config.resize_dimensions();

        let post = config.postprocessing.as_ref();
        let conf_threshold = post.and_then(|p| p.confidence_threshold).unwrap_or(0.25);
        let iou_threshold = post.and_then(|p| p.iou_threshold).unwrap_or(0.45);
        let apply_sigmoid = post.and_then(|p| p.apply_sigmoid).unwrap_or(true);
        let output_layout = post
            .and_then(|p| p.output_layout.clone())
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

/// Convert HandwritingDetParams to shared YoloParams.
impl From<&HandwritingDetParams> for crate::yolo_utils::YoloParams {
    fn from(p: &HandwritingDetParams) -> Self {
        Self {
            target_size: p.target_size,
            conf_threshold: p.conf_threshold,
            iou_threshold: p.iou_threshold,
            apply_sigmoid: p.apply_sigmoid,
            output_layout: p.output_layout.clone(),
        }
    }
}

/// Detect handwriting regions in an image using YOLOv8.
///
/// This function uses the same YOLOv8 detection framework as formula detection,
/// but with a model trained specifically for handwriting regions.
pub fn detect_handwriting(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    params: &HandwritingDetParams,
) -> Result<Vec<DetectionBox>> {
    let (letterboxed, scale, pad_x, pad_y) =
        latexsnipper_image::operations::letterbox(image, params.target_size);

    let pixels =
        latexsnipper_image::operations::normalize(&letterboxed, &params.mean, &params.std);

    let input = Tensor::float32(
        "images",
        vec![
            1,
            3,
            params.target_size as usize,
            params.target_size as usize,
        ],
        pixels,
    );

    let outputs = session.run(&[input])?;

    let output = outputs
        .first()
        .ok_or_else(|| SnipperError::Inference("No output tensor".into()))?;
    let raw_data = output
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output is not float32".into()))?;
    let shape = output.shape();

    // Use shared YOLO decoder
    let yolo_params = crate::yolo_utils::YoloParams::from(params);
    let boxes = decode_yolo_output(raw_data, shape, scale, pad_x, pad_y, &yolo_params)?;

    // Assign class info
    let boxes: Vec<DetectionBox> = boxes
        .into_iter()
        .map(|mut b| {
            b.class_id = 0;
            b.class_name = "handwriting".to_string();
            b
        })
        .collect();

    // Sort by confidence for better NMS
    let mut sorted_boxes = boxes;
    sorted_boxes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    // Apply NMS
    let nms_boxes = nms(sorted_boxes, 0.5);

    // Limit to reasonable number of detections
    let mut final_boxes = nms_boxes;
    final_boxes.truncate(50);

    Ok(final_boxes)
}

/// Filter handwriting detections by size and confidence.
pub fn filter_handwriting_detections(
    boxes: &mut Vec<DetectionBox>,
    min_area: f32,
    min_confidence: f32,
) {
    boxes.retain(|b| {
        let area = b.rect.width * b.rect.height;
        area >= min_area && b.confidence >= min_confidence
    });
}
