use latexsnipper_ast::Rect;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

/// A detected symbol in a formula image.
#[derive(Debug, Clone)]
pub struct SymbolDetection {
    /// The recognized symbol text.
    pub symbol: String,
    /// Bounding box of the symbol.
    pub rect: Rect,
    /// Confidence score.
    pub confidence: f32,
}

/// Symbol detection parameters.
#[derive(Debug, Clone)]
pub struct SymbolDetParams {
    pub target_size: u32,
    pub conf_threshold: f32,
    pub iou_threshold: f32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl Default for SymbolDetParams {
    fn default() -> Self {
        Self {
            target_size: 384,
            conf_threshold: 0.3,
            iou_threshold: 0.4,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
        }
    }
}

/// Detect individual symbols in a formula image.
///
/// This function uses a detection model to find bounding boxes
/// for each symbol in the formula, enabling symbol-level alignment
/// with the parsed LaTeX structure.
pub fn detect_symbols(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    params: &SymbolDetParams,
) -> Result<Vec<SymbolDetection>> {
    // Preprocess image
    let resized = latexsnipper_image::operations::resize(
        image,
        params.target_size,
        params.target_size,
    );
    let pixels = latexsnipper_image::operations::normalize(
        &resized,
        &params.mean,
        &params.std,
    );

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

    // Parse detection output
    let scale = params.target_size as f32 / image.width().max(image.height()) as f32;
    let detections = parse_detections(raw_data, shape, scale, params)?;

    Ok(detections)
}

/// Parse detection output tensor into symbol detections.
fn parse_detections(
    data: &[f32],
    shape: &[usize],
    scale: f32,
    params: &SymbolDetParams,
) -> Result<Vec<SymbolDetection>> {
    let mut detections = Vec::new();

    // Expected shape: [1, num_detections, 6] or [1, 6, num_detections]
    // Each detection: [cx, cy, w, h, confidence, class_id]
    let num_detections = if shape.len() == 3 {
        shape[1].min(shape[2])
    } else if shape.len() == 2 {
        shape[0].min(shape[1])
    } else {
        return Ok(detections);
    };

    let is_row_major = shape.len() == 3 && shape[2] == 6;

    for i in 0..num_detections {
        let (cx, cy, w, h, conf) = if is_row_major {
            let base = i * 6;
            if base + 5 >= data.len() {
                break;
            }
            (data[base], data[base + 1], data[base + 2], data[base + 3], data[base + 4])
        } else {
            let n = num_detections;
            (data[i], data[n + i], data[2 * n + i], data[3 * n + i], data[4 * n + i])
        };

        if conf < params.conf_threshold {
            continue;
        }

        // Convert to image coordinates
        let x1 = (cx - w / 2.0) / scale;
        let y1 = (cy - h / 2.0) / scale;
        let bw = w / scale;
        let bh = h / scale;

        detections.push(SymbolDetection {
            symbol: String::new(), // Will be filled by recognition
            rect: Rect::new(x1.max(0.0), y1.max(0.0), bw, bh),
            confidence: conf,
        });
    }

    // Sort by x position (reading order)
    detections.sort_by(|a, b| {
        a.rect
            .x
            .partial_cmp(&b.rect.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply NMS
    let detections = nms(detections, params.iou_threshold);

    Ok(detections)
}

/// Non-maximum suppression for symbol detections.
fn nms(mut detections: Vec<SymbolDetection>, iou_threshold: f32) -> Vec<SymbolDetection> {
    detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    let mut keep = Vec::new();
    let mut suppressed = vec![false; detections.len()];

    for i in 0..detections.len() {
        if suppressed[i] {
            continue;
        }
        keep.push(detections[i].clone());

        for j in (i + 1)..detections.len() {
            if suppressed[j] {
                continue;
            }
            if detections[i].rect.iou(&detections[j].rect) > iou_threshold {
                suppressed[j] = true;
            }
        }
    }

    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_detection_sorting() {
        let mut detections = vec![
            SymbolDetection {
                symbol: "b".into(),
                rect: Rect::new(100.0, 0.0, 10.0, 10.0),
                confidence: 0.9,
            },
            SymbolDetection {
                symbol: "a".into(),
                rect: Rect::new(10.0, 0.0, 10.0, 10.0),
                confidence: 0.95,
            },
        ];

        detections.sort_by(|a, b| {
            a.rect
                .x
                .partial_cmp(&b.rect.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        assert_eq!(detections[0].symbol, "a");
        assert_eq!(detections[1].symbol, "b");
    }
}
