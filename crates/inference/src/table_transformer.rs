//! Table Transformer (Microsoft DETR-based) detection and structure recognition.
//!
//! Implements inference and grid-building for:
//! - `microsoft/table-transformer-detection` — detects table regions in pages
//! - `microsoft/table-transformer-structure-recognition` — detects rows/cols/cells in tables
//!
//! Both models share the same DETR inference pipeline (`recognize_table_transformer`).
//! The detection model outputs 3 classes: no_object, table, table_rotated (15 queries).
//! The structure model outputs 7 classes: no_object, table, column, row, column_header,
//! row_header, spanning_cell (125 queries).

use crate::types::GridCell;
use latexsnipper_ast::Rect;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

/// A single DETR query output — class label, confidence, bounding box.
#[derive(Debug, Clone)]
pub struct TableTransformerDetection {
    /// Class label
    ///   Detection model: 0=no_object, 1=table, 2=table_rotated
    ///   Structure model: 0=no_object, 1=table, 2=column, 3=row,
    ///                    4=column_header, 5=row_header, 6=spanning_cell
    pub class_id: u32,
    /// Confidence score (softmax probability)
    pub score: f32,
    /// Bounding box in (x1, y1, x2, y2) normalized to [0,1], then scaled to image pixels
    pub bbox: [f32; 4],
}

/// Structure recognition labels (7 classes including no_object at index 0).
pub const TABLE_STRUCTURE_LABELS: &[&str] = &[
    "no_object",                  // 0
    "table",                      // 1
    "table column",               // 2
    "table row",                  // 3
    "table column header",        // 4
    "table projected row header", // 5
    "table spanning cell",        // 6
];

/// Detection labels (3 classes including no_object at index 0).
pub const TABLE_DETECTION_LABELS: &[&str] = &[
    "no_object",     // 0
    "table",         // 1
    "table rotated", // 2
];

/// Run a Table Transformer model (detection or structure) and return detections.
///
/// Both models share the same DETR architecture: resize longest edge to 1000,
/// ImageNet normalization, softmax over classes, cxcywh→xyxy conversion.
///
/// # Arguments
/// * `image` — input image (usually the full page or a table crop)
/// * `session` — ONNX inference session for the model
///
/// # Returns
/// Detections filtered by confidence ≥ 0.7 and deduplicated by NMS within each class.
pub fn recognize_table_transformer(
    image: &SnipperImage,
    session: &dyn InferenceSession,
) -> Result<Vec<TableTransformerDetection>> {
    let (w, h) = (image.width() as f32, image.height() as f32);
    let max_dim = w.max(h);
    let scale = 1000.0 / max_dim;
    let new_w = (w * scale).round() as u32;
    let new_h = (h * scale).round() as u32;
    let resized = latexsnipper_image::operations::resize(image, new_w, new_h);
    let mean = [0.485f32, 0.456, 0.406];
    let std_dev = [0.229f32, 0.224, 0.225];
    let normalized = latexsnipper_image::operations::normalize(&resized, &mean, &std_dev);

    let input = Tensor::float32(
        "pixel_values",
        vec![1, 3, new_h as usize, new_w as usize],
        normalized,
    );

    let outputs = session.run(&[input])?;

    if outputs.len() < 2 {
        return Err(SnipperError::Inference(
            "TableTransformer expected 2 outputs".into(),
        ));
    }

    let logits = outputs[0]
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("logits not float32".into()))?;
    let pred_boxes = outputs[1]
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("pred_boxes not float32".into()))?;

    let logits_shape = outputs[0].shape();
    let num_queries = if logits_shape.len() >= 2 {
        logits_shape[1]
    } else {
        0
    };
    let num_classes = if logits_shape.len() >= 3 {
        logits_shape[2]
    } else {
        0
    };

    if num_queries == 0 || num_classes == 0 {
        return Err(SnipperError::Inference("Invalid logits shape".into()));
    }

    let mut detections = Vec::new();

    // Debug: print first query's raw logits
    if logits.len() >= 7 {
        eprintln!("TATR DEBUG: Query 0 logits: {:?}", &logits[0..7]);
    }
    eprintln!(
        "TATR DEBUG: logits_shape={:?}, total_logits={}, total_boxes={}",
        logits_shape,
        logits.len(),
        pred_boxes.len()
    );

    for q in 0..num_queries {
        let logits_offset = q * num_classes;
        let box_offset = q * 4;

        if box_offset + 4 > pred_boxes.len() || logits_offset + num_classes > logits.len() {
            break;
        }

        // Softmax
        let max_logit = logits[logits_offset..logits_offset + num_classes]
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = logits[logits_offset..logits_offset + num_classes]
            .iter()
            .map(|&x| (x - max_logit).exp())
            .sum();

        let mut best_class = 0u32;
        let mut best_score = 0.0f32;
        for c in 0..num_classes {
            let prob = (logits[logits_offset + c] - max_logit).exp() / exp_sum;
            if prob > best_score {
                best_score = prob;
                best_class = c as u32;
            }
        }

        if best_score < 0.7 {
            continue;
        }

        // Convert cxcywh to xyxy (normalized)
        let cx = pred_boxes[box_offset];
        let cy = pred_boxes[box_offset + 1];
        let bw = pred_boxes[box_offset + 2];
        let bh = pred_boxes[box_offset + 3];

        let x1 = (cx - bw / 2.0).max(0.0);
        let y1 = (cy - bh / 2.0).max(0.0);
        let x2 = (cx + bw / 2.0).min(1.0);
        let y2 = (cy + bh / 2.0).min(1.0);

        if x2 > x1 && y2 > y1 {
            detections.push(TableTransformerDetection {
                class_id: best_class,
                score: best_score,
                bbox: [x1, y1, x2, y2],
            });
        }
    }

    // Remove full-image detections (>80% coverage), except for table classes (0-2)
    let mut filtered: Vec<TableTransformerDetection> = Vec::new();
    for det in detections {
        let bw = det.bbox[2] - det.bbox[0];
        let bh = det.bbox[3] - det.bbox[1];
        if det.class_id > 2 && bw > 0.8 && bh > 0.8 {
            continue;
        }
        filtered.push(det);
    }

    // Sort by score, deduplicate by IoU within same class
    filtered.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut keep = Vec::new();
    for det in filtered {
        let is_duplicate = keep.iter().any(|k: &TableTransformerDetection| {
            k.class_id == det.class_id && compute_iou(&k.bbox, &det.bbox) > 0.3
        });
        if !is_duplicate {
            keep.push(det);
        }
    }

    // Sort by y-coordinate for reading order
    keep.sort_by(|a, b| {
        a.bbox[1]
            .partial_cmp(&b.bbox[1])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Scale to pixel coordinates
    for det in &mut keep {
        det.bbox[0] *= w;
        det.bbox[1] *= h;
        det.bbox[2] *= w;
        det.bbox[3] *= h;
    }

    Ok(keep)
}

fn compute_iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);

    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area_a = (a[2] - a[0]) * (a[3] - a[1]);
    let area_b = (b[2] - b[0]) * (b[3] - b[1]);
    let union = area_a + area_b - inter;

    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

// GridCell is now in types.rs

/// Build a cell grid from structure model detections.
///
/// The structure model outputs 7 classes:
///   0: no_object   1: table          2: table column
///   3: table row   4: column header   5: row header
///   6: spanning cell
///
/// Algorithm:
/// 1. Extract row boundaries from class 3 (table row) detections
/// 2. Extract column boundaries from class 2 (table column) detections
/// 3. Intersect row/col boundaries to form a dense grid
/// 4. Spanning cells (class 6) set rowspan/colspan on the covered grid area
pub fn build_grid_from_detections(
    detections: &[TableTransformerDetection],
    img_w: f32,
    img_h: f32,
) -> Vec<GridCell> {
    let mut row_dets: Vec<&TableTransformerDetection> =
        detections.iter().filter(|d| d.class_id == 3).collect();
    row_dets.sort_by(|a, b| a.bbox[1].partial_cmp(&b.bbox[1]).unwrap());

    let mut col_dets: Vec<&TableTransformerDetection> =
        detections.iter().filter(|d| d.class_id == 2).collect();
    col_dets.sort_by(|a, b| a.bbox[0].partial_cmp(&b.bbox[0]).unwrap());

    let spanning: Vec<&TableTransformerDetection> =
        detections.iter().filter(|d| d.class_id == 6).collect();

    let mut y_edges: Vec<f32> = Vec::new();
    let mut x_edges: Vec<f32> = Vec::new();

    let use_row_col = row_dets.len() >= 3 && col_dets.len() >= 3;

    if use_row_col {
        y_edges.push(0.0);
        for det in &row_dets {
            for &edge in &[det.bbox[1], det.bbox[3]] {
                if edge > 0.0 && edge < img_h && !y_edges.iter().any(|&e| (e - edge).abs() < 4.0) {
                    y_edges.push(edge);
                }
            }
        }

        x_edges.push(0.0);
        for det in &col_dets {
            for &edge in &[det.bbox[0], det.bbox[2]] {
                if edge > 0.0 && edge < img_w && !x_edges.iter().any(|&e| (e - edge).abs() < 4.0) {
                    x_edges.push(edge);
                }
            }
        }
    } else if spanning.len() >= 2 {
        let mut raw_y: Vec<f32> = Vec::new();
        let mut raw_x: Vec<f32> = Vec::new();
        for sp in &spanning {
            raw_y.push(sp.bbox[1]);
            raw_y.push(sp.bbox[3]);
            raw_x.push(sp.bbox[0]);
            raw_x.push(sp.bbox[2]);
        }
        raw_y.sort_by(|a, b| a.partial_cmp(b).unwrap());
        raw_x.sort_by(|a, b| a.partial_cmp(b).unwrap());

        for &edge in &raw_y {
            if edge > 0.0 && edge < img_h && !y_edges.iter().any(|&e| (e - edge).abs() < 4.0) {
                y_edges.push(edge);
            }
        }
        for &edge in &raw_x {
            if edge > 0.0 && edge < img_w && !x_edges.iter().any(|&e| (e - edge).abs() < 4.0) {
                x_edges.push(edge);
            }
        }
    }

    y_edges.push(img_h);
    y_edges.sort_by(|a, b| a.partial_cmp(b).unwrap());
    x_edges.push(img_w);
    x_edges.sort_by(|a, b| a.partial_cmp(b).unwrap());

    if y_edges.len() < 2 || x_edges.len() < 2 {
        return build_fallback_grid(detections, img_w, img_h);
    }

    let num_rows = y_edges.len() - 1;
    let num_cols = x_edges.len() - 1;

    let mut grid_cells: Vec<GridCell> = Vec::new();
    for r in 0..num_rows {
        for c in 0..num_cols {
            let x1 = x_edges[c];
            let y1 = y_edges[r];
            let x2 = x_edges[c + 1];
            let y2 = y_edges[r + 1];
            let w = x2 - x1;
            let h = y2 - y1;
            if w > 2.0 && h > 2.0 {
                grid_cells.push(GridCell {
                    row: r,
                    col: c,
                    rowspan: 1,
                    colspan: 1,
                    rect: Rect::new(x1, y1, w, h),
                });
            }
        }
    }

    let mut skip_indices: Vec<usize> = Vec::new();
    let mut extra_cells: Vec<GridCell> = Vec::new();

    for sp in &spanning {
        let sx1 = sp.bbox[0];
        let sy1 = sp.bbox[1];
        let sx2 = sp.bbox[2];
        let sy2 = sp.bbox[3];

        let (mut start_row, mut end_row) = (None, 0);
        let (mut start_col, mut end_col) = (None, 0);

        for r in 0..num_rows {
            if sy1 < y_edges[r + 1] && sy2 > y_edges[r] {
                if start_row.is_none() {
                    start_row = Some(r);
                }
                end_row = r;
            }
        }
        for c in 0..num_cols {
            if sx1 < x_edges[c + 1] && sx2 > x_edges[c] {
                if start_col.is_none() {
                    start_col = Some(c);
                }
                end_col = c;
            }
        }

        if let (Some(sr), Some(sc)) = (start_row, start_col) {
            let rspan = end_row - sr + 1;
            let cspan = end_col - sc + 1;
            let avg_cell_w = (x_edges[sc + 1] - x_edges[sc]).max(1.0);
            let avg_cell_h = (y_edges[sr + 1] - y_edges[sr]).max(1.0);
            let is_real_merge = (rspan * cspan >= 3)
                || (rspan >= 2 && (sx2 - sx1) > avg_cell_w * 1.8)
                || (cspan >= 2 && (sy2 - sy1) > avg_cell_h * 1.8);

            if is_real_merge {
                for r in sr..=end_row {
                    for c in sc..=end_col {
                        let idx = r * num_cols + c;
                        if !skip_indices.contains(&idx) {
                            skip_indices.push(idx);
                        }
                    }
                }
                extra_cells.push(GridCell {
                    row: sr,
                    col: sc,
                    rowspan: rspan as u32,
                    colspan: cspan as u32,
                    rect: Rect::new(sx1, sy1, sx2 - sx1, sy2 - sy1),
                });
            }
        }
    }

    let mut cells: Vec<GridCell> = Vec::new();
    cells.extend(extra_cells.clone());

    for (idx, gc) in grid_cells.iter().enumerate() {
        if skip_indices.contains(&idx) {
            continue;
        }
        let cx = gc.rect.x + gc.rect.width / 2.0;
        let cy = gc.rect.y + gc.rect.height / 2.0;
        let inside_span = extra_cells.iter().any(|ec| {
            cx >= ec.rect.x
                && cx <= ec.rect.x + ec.rect.width
                && cy >= ec.rect.y
                && cy <= ec.rect.y + ec.rect.height
        });
        if inside_span {
            continue;
        }
        cells.push(gc.clone());
    }

    cells
}

fn build_fallback_grid(
    detections: &[TableTransformerDetection],
    img_w: f32,
    img_h: f32,
) -> Vec<GridCell> {
    let content: Vec<&TableTransformerDetection> = detections
        .iter()
        .filter(|d| d.class_id != 0 && d.class_id != 1 && d.score > 0.3)
        .collect();

    if content.is_empty() {
        return vec![GridCell {
            row: 0,
            col: 0,
            rowspan: 1,
            colspan: 1,
            rect: Rect::new(0.0, 0.0, img_w, img_h),
        }];
    }

    let span_only: Vec<&&TableTransformerDetection> =
        content.iter().filter(|d| d.class_id == 6).collect();
    if !span_only.is_empty() {
        let mut cells: Vec<GridCell> = span_only
            .iter()
            .enumerate()
            .map(|(i, d)| GridCell {
                row: i,
                col: 0,
                rowspan: 1,
                colspan: 1,
                rect: Rect::new(
                    d.bbox[0],
                    d.bbox[1],
                    d.bbox[2] - d.bbox[0],
                    d.bbox[3] - d.bbox[1],
                ),
            })
            .collect();
        cells.sort_by(|a, b| a.rect.y.partial_cmp(&b.rect.y).unwrap());
        return cells;
    }

    let mut sorted = content.clone();
    sorted.sort_by(|a, b| {
        a.bbox[1]
            .partial_cmp(&b.bbox[1])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.bbox[0]
                    .partial_cmp(&b.bbox[0])
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut rows: Vec<Vec<&TableTransformerDetection>> = Vec::new();
    for &det in &sorted {
        let det_cy = (det.bbox[1] + det.bbox[3]) / 2.0;
        let placed = rows.iter_mut().find(|row| {
            let row_cy = row
                .iter()
                .map(|d| (d.bbox[1] + d.bbox[3]) / 2.0)
                .sum::<f32>()
                / row.len() as f32;
            (det_cy - row_cy).abs() < 10.0
        });
        if let Some(row) = placed {
            row.push(det);
        } else {
            rows.push(vec![det]);
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| a.bbox[0].partial_cmp(&b.bbox[0]).unwrap());
    }

    let mut cells = Vec::new();
    for (ri, row) in rows.iter().enumerate() {
        for (ci, det) in row.iter().enumerate() {
            let w = det.bbox[2] - det.bbox[0];
            let h = det.bbox[3] - det.bbox[1];
            if w > 2.0 && h > 2.0 {
                cells.push(GridCell {
                    row: ri,
                    col: ci,
                    rowspan: 1,
                    colspan: 1,
                    rect: Rect::new(det.bbox[0], det.bbox[1], w, h),
                });
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structure_labels() {
        assert_eq!(TABLE_STRUCTURE_LABELS.len(), 7);
        assert_eq!(TABLE_STRUCTURE_LABELS[0], "no_object");
        assert_eq!(TABLE_STRUCTURE_LABELS[3], "table row");
    }

    #[test]
    fn test_detection_labels() {
        assert_eq!(TABLE_DETECTION_LABELS.len(), 3);
        assert_eq!(TABLE_DETECTION_LABELS[0], "no_object");
        assert_eq!(TABLE_DETECTION_LABELS[1], "table");
    }

    #[test]
    fn test_iou() {
        let a = [0.0, 0.0, 1.0, 1.0];
        let b = [0.5, 0.5, 1.5, 1.5];
        let iou = compute_iou(&a, &b);
        assert!(
            (iou - 1.0 / 7.0).abs() < 0.01,
            "IoU should be ~0.143, got {}",
            iou
        );
    }

    #[test]
    fn test_cxcywh_to_xyxy() {
        let cx = 0.5f32;
        let cy = 0.5f32;
        let bw = 0.2f32;
        let bh = 0.2f32;
        let x1 = cx - bw / 2.0;
        let y1 = cy - bh / 2.0;
        let x2 = cx + bw / 2.0;
        let y2 = cy + bh / 2.0;
        assert!((x1 - 0.4).abs() < 0.001);
        assert!((y1 - 0.4).abs() < 0.001);
        assert!((x2 - 0.6).abs() < 0.001);
        assert!((y2 - 0.6).abs() < 0.001);
    }
}
