use latexsnipper_ast::Rect;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::DetectionBox;

/// Text detection parameters loaded from config.json.
#[derive(Debug, Clone)]
pub struct TextDetParams {
    pub max_side: u32,
    pub stride: u32,
    pub det_threshold: f32,
    pub box_threshold: f32,
    pub unclip_ratio: f32,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl Default for TextDetParams {
    fn default() -> Self {
        Self {
            max_side: 960,
            stride: 32,
            det_threshold: 0.3,
            box_threshold: 0.5,
            unclip_ratio: 1.6,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
        }
    }
}

/// Detect text regions using DBNet.
pub fn detect_text(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    params: &TextDetParams,
) -> Result<Vec<DetectionBox>> {
    let (processed, orig_w, orig_h, scale) = preprocess(image, params);

    let input = Tensor::float32(
        "x",
        vec![
            1,
            3,
            processed.height() as usize,
            processed.width() as usize,
        ],
        latexsnipper_image::operations::normalize(&processed, &params.mean, &params.std),
    );
    let outputs = session.run(&[input])?;

    let output = outputs
        .first()
        .ok_or_else(|| SnipperError::Inference("No output".into()))?;
    let prob_map = output
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output not float32".into()))?;
    let map_shape = output.shape().to_vec();

    let boxes = postprocess(prob_map, &map_shape, orig_w, orig_h, scale, params)?;

    Ok(boxes)
}

fn preprocess(image: &SnipperImage, params: &TextDetParams) -> (SnipperImage, u32, u32, f32) {
    let w = image.width();
    let h = image.height();
    let max_side = w.max(h);

    let scale = if max_side > params.max_side {
        params.max_side as f32 / max_side as f32
    } else {
        1.0
    };

    let new_w = (w as f32 * scale).ceil() as u32;
    let new_h = (h as f32 * scale).ceil() as u32;

    let new_w = new_w.div_ceil(params.stride) * params.stride;
    let new_h = new_h.div_ceil(params.stride) * params.stride;

    let resized = latexsnipper_image::operations::resize(image, new_w, new_h);
    let padded = latexsnipper_image::operations::pad_to_stride(&resized, params.stride);

    (padded, w, h, scale)
}

fn postprocess(
    prob_map: &[f32],
    shape: &[usize],
    orig_w: u32,
    orig_h: u32,
    scale: f32,
    params: &TextDetParams,
) -> Result<Vec<DetectionBox>> {
    let map_h = shape[2];
    let map_w = shape[3];

    let mut binary = vec![0u8; map_h * map_w];
    for i in 0..map_h * map_w {
        binary[i] = if prob_map[i] > params.det_threshold {
            1
        } else {
            0
        };
    }

    let contours = find_contours(&binary, map_w, map_h);

    let mut boxes = Vec::new();

    for contour in &contours {
        let area = polygon_area(contour);
        let perimeter = polygon_perimeter(contour);

        if perimeter <= 0.0 || area < 1.0 {
            continue;
        }

        let distance = area * params.unclip_ratio / perimeter;
        let expanded = expand_contour(contour, distance);

        let (min_x, min_y, max_x, max_y) = bounding_box(&expanded);

        let x1 = (min_x as f32 / scale).max(0.0);
        let y1 = (min_y as f32 / scale).max(0.0);
        let x2 = (max_x as f32 / scale).min(orig_w as f32);
        let y2 = (max_y as f32 / scale).min(orig_h as f32);

        let w = x2 - x1;
        let h = y2 - y1;

        if w < 1.0 || h < 1.0 {
            continue;
        }

        let avg_score = average_score(prob_map, map_w, map_h, min_x, min_y, max_x, max_y);
        if avg_score < params.box_threshold {
            continue;
        }

        boxes.push(DetectionBox {
            rect: Rect::new(x1, y1, w, h),
            confidence: avg_score,
            class_id: 0,
            class_name: "text".into(),
        });
    }

    merge_boxes(&mut boxes);

    Ok(boxes)
}

fn find_contours(binary: &[u8], width: usize, height: usize) -> Vec<Vec<(i32, i32)>> {
    let mut contours = Vec::new();
    let mut visited = vec![false; width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if binary[idx] == 1 && !visited[idx] {
                if let Some(contour) = trace_contour(binary, width, height, x, y, &mut visited) {
                    if contour.len() >= 4 {
                        contours.push(contour);
                    }
                }
            }
        }
    }

    contours
}

fn trace_contour(
    binary: &[u8],
    width: usize,
    height: usize,
    start_x: usize,
    start_y: usize,
    visited: &mut [bool],
) -> Option<Vec<(i32, i32)>> {
    let mut contour = Vec::new();
    let dirs = [
        (0, 1),
        (1, 1),
        (1, 0),
        (1, -1),
        (0, -1),
        (-1, -1),
        (-1, 0),
        (-1, 1),
    ];

    let mut x = start_x as i32;
    let mut y = start_y as i32;
    let mut dir = 0;

    for _ in 0..10000 {
        contour.push((x, y));
        visited[(y as usize) * width + (x as usize)] = true;

        let start_dir = (dir + 5) % 8;
        let mut found = false;

        for i in 0..8 {
            let d = (start_dir + i) % 8;
            let nx = x + dirs[d].0;
            let ny = y + dirs[d].1;

            if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                let nidx = (ny as usize) * width + (nx as usize);
                if binary[nidx] == 1 {
                    x = nx;
                    y = ny;
                    dir = d;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            break;
        }

        if x == start_x as i32 && y == start_y as i32 && contour.len() > 3 {
            break;
        }
    }

    if contour.len() > 3 {
        Some(contour)
    } else {
        None
    }
}

fn polygon_area(points: &[(i32, i32)]) -> f32 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0f32;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].0 as f32 * points[j].1 as f32;
        area -= points[j].0 as f32 * points[i].1 as f32;
    }
    area.abs() / 2.0
}

fn polygon_perimeter(points: &[(i32, i32)]) -> f32 {
    let n = points.len();
    if n < 2 {
        return 0.0;
    }
    let mut perim = 0.0f32;
    for i in 0..n {
        let j = (i + 1) % n;
        let dx = points[i].0 - points[j].0;
        let dy = points[i].1 - points[j].1;
        perim += ((dx * dx + dy * dy) as f32).sqrt();
    }
    perim
}

fn expand_contour(points: &[(i32, i32)], distance: f32) -> Vec<(i32, i32)> {
    let cx: f32 = points.iter().map(|p| p.0 as f32).sum::<f32>() / points.len() as f32;
    let cy: f32 = points.iter().map(|p| p.1 as f32).sum::<f32>() / points.len() as f32;

    points
        .iter()
        .map(|&(px, py)| {
            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 {
                (
                    (px as f32 + dx / len * distance) as i32,
                    (py as f32 + dy / len * distance) as i32,
                )
            } else {
                (px, py)
            }
        })
        .collect()
}

fn bounding_box(points: &[(i32, i32)]) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (min_x, min_y, max_x, max_y)
}

fn average_score(
    map: &[f32],
    width: usize,
    height: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) -> f32 {
    let x1 = x1.max(0) as usize;
    let y1 = y1.max(0) as usize;
    let x2 = (x2 as usize).min(width);
    let y2 = (y2 as usize).min(height);

    if x1 >= x2 || y1 >= y2 {
        return 0.0;
    }

    let mut sum = 0.0f32;
    let mut count = 0;
    for y in y1..y2 {
        for x in x1..x2 {
            sum += map[y * width + x];
            count += 1;
        }
    }

    if count > 0 {
        sum / count as f32
    } else {
        0.0
    }
}

fn merge_boxes(boxes: &mut Vec<DetectionBox>) {
    if boxes.is_empty() {
        return;
    }

    // Sort by Y position
    boxes.sort_by(|a, b| a.rect.y.partial_cmp(&b.rect.y).unwrap());

    let mut merged = Vec::new();
    let mut used = vec![false; boxes.len()];

    for i in 0..boxes.len() {
        if used[i] {
            continue;
        }

        let mut current = boxes[i].clone();
        used[i] = true;

        // Merge with nearby boxes on the same line
        for j in (i + 1)..boxes.len() {
            if used[j] {
                continue;
            }

            let other = &boxes[j];

            // Check if on same line: Y centers within 1.5x average height
            let avg_height = (current.rect.height + other.rect.height) / 2.0;
            let y_center_diff = (current.rect.center_y() - other.rect.center_y()).abs();

            if y_center_diff > avg_height * 1.5 {
                continue; // Too far vertically, skip
            }

            // Check X overlap or proximity
            let x_gap = if other.rect.x > current.rect.right() {
                other.rect.x - current.rect.right()
            } else if current.rect.x > other.rect.right() {
                current.rect.x - other.rect.right()
            } else {
                0.0 // Overlapping
            };

            // Merge if close enough (gap < 50% of average width)
            let avg_width = (current.rect.width + other.rect.width) / 2.0;
            if x_gap < avg_width * 0.5 {
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
