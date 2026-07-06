use latexsnipper_ast::{Point, Quad};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::DetectionBox;

/// Text detection parameters loaded from config.json.
#[derive(Debug, Clone)]
pub struct TextDetParams {
    pub input_name: String,
    pub output_name: String,
    pub color_format: String,
    pub max_side: u32,
    pub stride: u32,
    pub det_threshold: f32,
    pub box_threshold: f32,
    pub unclip_ratio: f32,
    pub max_candidates: usize,
    pub box_type: String,
    pub score_mode: String,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl TextDetParams {
    /// Compile TextDetParams from ModelConfig.
    pub fn from_config(config: &latexsnipper_model::ModelConfig) -> Self {
        let mut params = Self::default();

        // Input/output tensor names
        if let Some(input) = &config.input {
            params.input_name = input.name.clone();
        }
        if let Some(output) = &config.output {
            params.output_name = output.name.clone();
        }

        // Color format
        let cf = config.color_format().to_lowercase();
        if cf == "bgr" || cf == "rgb" {
            params.color_format = cf;
        }

        // Preprocessing: max_side only from input shape when config is clearly DBNet-specific
        // (has postprocessing config or model_type is "dbnet").
        // Otherwise keep default 960, since fallback configs (e.g. from_paddle_inference_dir)
        // may have recognition-shaped inputs like [1,3,48,3200] that would set wrong max_side.
        let has_dbnet_config = config.postprocessing.is_some() || config.model_type == "dbnet";
        if has_dbnet_config {
            if let Some(input) = &config.input {
                if input.shape.len() == 4 {
                    if let Some(&h) = input.shape.get(2) {
                        if h > 0 && h != -1 {
                            params.max_side = h as u32;
                        }
                    }
                    if let Some(&w) = input.shape.get(3) {
                        if w > 0 && w != -1 {
                            params.max_side = params.max_side.max(w as u32);
                        }
                    }
                }
            }
        }

        // Postprocessing params
        if let Some(post) = &config.postprocessing {
            if let Some(th) = post.threshold {
                params.det_threshold = th;
            }
            if let Some(bt) = post.box_threshold {
                params.box_threshold = bt;
            }
            if let Some(ur) = post.unclip_ratio {
                params.unclip_ratio = ur;
            }
            if let Some(mc) = post.max_candidates {
                params.max_candidates = mc;
            }
            params.box_type = post.dbnet_box_type().to_string();
            params.score_mode = post.dbnet_score_mode().to_string();
        }

        // Divisible by from preprocessing
        if let Some(pre) = &config.preprocessing {
            if let Some(d) = pre.divisible_by {
                params.stride = d;
            }
        }

        // Normalization
        let mean = config.normalization_mean();
        if mean.len() == 3 {
            params.mean = [mean[0], mean[1], mean[2]];
        }
        let std = config.normalization_std();
        if std.len() == 3 {
            params.std = [std[0], std[1], std[2]];
        }

        params
    }
}

impl Default for TextDetParams {
    fn default() -> Self {
        Self {
            input_name: "x".into(),
            output_name: "output".into(),
            color_format: "RGB".into(),
            max_side: 960,
            stride: 32,
            det_threshold: 0.3,
            box_threshold: 0.5,
            unclip_ratio: 1.6,
            max_candidates: 1000,
            box_type: "quad".into(),
            score_mode: "fast".into(),
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
        &params.input_name,
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

    // Debug
    let above_thresh: usize = prob_map
        .iter()
        .filter(|&&v| v > params.det_threshold)
        .count();
    eprintln!(
        "text-det debug: input={}x{}, output shape={:?}, prob_map len={}, above_threshold={}, boxes={}",
        processed.width(), processed.height(), map_shape, prob_map.len(), above_thresh, boxes.len()
    );

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

    // Collect connected components as actual contour edge points
    let contours = find_contours(&binary, map_w, map_h);

    // Debug
    eprintln!(
        "text-det postprocess: map={}x{}, binary ones={}, contours={}",
        map_w,
        map_h,
        binary.iter().filter(|&&b| b == 1).count(),
        contours.len()
    );

    let mut boxes = Vec::new();

    for contour in &contours {
        let area = polygon_area(contour);
        let perimeter = polygon_perimeter(contour);

        if perimeter <= 0.0 || area < 1.0 {
            continue;
        }

        let distance = area * params.unclip_ratio / perimeter;
        let expanded = expand_contour(contour, distance);

        // Compute min-area bounding quad from the expanded contour
        let quad = min_area_quad(&expanded);

        let scaled_quad = quad.scale(1.0 / scale, 1.0 / scale);
        let clamped_quad = scaled_quad.clamp(orig_w as f32, orig_h as f32);

        // Compute bounding rect from quad for backward compatibility
        let brect = clamped_quad.bounding_rect();

        if brect.width < 1.0 || brect.height < 1.0 {
            continue;
        }

        // Compute box score using the quad on the probability map
        // The quad is in prob-map coordinates before scaling
        let avg_score = if params.score_mode == "slow" {
            // Polygon mask score (slower but more accurate)
            polygon_average_score(prob_map, map_w, map_h, &quad, scale)
        } else {
            // Bounding-box mask score (fast mode)
            let bbox = quad.bounding_rect();
            average_score(
                prob_map, map_w, map_h,
                bbox.x as i32, bbox.y as i32,
                bbox.right() as i32, bbox.bottom() as i32,
            )
        };

        if avg_score < params.box_threshold {
            if boxes.len() < 5 {
                eprintln!(
                    "  filtered contour: area={:.1} perimeter={:.1} avg_score={:.3} < box_thresh={}",
                    area, perimeter, avg_score, params.box_threshold
                );
            }
            continue;
        }

        boxes.push(DetectionBox::quad(
            clamped_quad,
            avg_score,
            0,
            "text".into(),
        ));
    }

    // Sort by confidence (highest first) up to max_candidates
    boxes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    boxes.truncate(params.max_candidates);

    Ok(boxes)
}

/// Find connected components and return their edge contour pixels.
fn find_contours(binary: &[u8], width: usize, height: usize) -> Vec<Vec<(i32, i32)>> {
    let mut visited = vec![false; width * height];
    let mut contours = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if binary[idx] == 1 && !visited[idx] {
                // BFS flood fill to find all component pixels
                let mut queue = std::collections::VecDeque::new();
                let mut points: Vec<(i32, i32)> = Vec::new();

                queue.push_back((x, y));
                visited[idx] = true;

                while let Some((cx, cy)) = queue.pop_front() {
                    points.push((cx as i32, cy as i32));

                    // 4-connected neighbors
                    for &(dx, dy) in &[(0i32, 1i32), (1, 0), (0, -1), (-1, 0)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            let nidx = ny as usize * width + nx as usize;
                            if binary[nidx] == 1 && !visited[nidx] {
                                visited[nidx] = true;
                                queue.push_back((nx as usize, ny as usize));
                            }
                        }
                    }
                }

                if points.len() < 4 {
                    continue;
                }

                // Collect boundary pixels (edge of the component)
                let boundary: Vec<(i32, i32)> = points
                    .iter()
                    .filter(|&&(px, py)| {
                        // A pixel is on the boundary if any 4-neighbor is outside the component
                        px == 0
                            || py == 0
                            || px >= width as i32 - 1
                            || py >= height as i32 - 1
                            || [(0i32, 1i32), (1, 0), (0, -1), (-1, 0)].iter().any(|&(dx, dy)| {
                                let nx = (px + dx) as usize;
                                let ny = (py + dy) as usize;
                                binary[ny * width + nx] == 0
                            })
                    })
                    .copied()
                    .collect();

                if boundary.len() >= 4 {
                    contours.push(boundary);
                }
            }
        }
    }

    contours
}

/// Compute the minimum-area bounding quadrilateral from a set of contour points.
/// Uses convex hull + rotating calipers for the minimum-area oriented bounding box.
fn min_area_quad(points: &[(i32, i32)]) -> Quad {
    let hull = convex_hull(points);
    if hull.len() < 3 {
        // Fallback to axis-aligned bounding box
        let (min_x, min_y, max_x, max_y) = bounding_box(points);
        return Quad::new(
            Point::new(min_x as f32, min_y as f32),
            Point::new(max_x as f32, min_y as f32),
            Point::new(max_x as f32, max_y as f32),
            Point::new(min_x as f32, max_y as f32),
        );
    }

    minimum_area_bounding_rect(&hull)
}

/// Andrew's monotone chain convex hull.
fn convex_hull(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    pts.dedup();

    if pts.len() < 3 {
        return pts;
    }

    let cross = |o: (i32, i32), a: (i32, i32), b: (i32, i32)| -> i64 {
        (a.0 as i64 - o.0 as i64) * (b.1 as i64 - o.1 as i64)
            - (a.1 as i64 - o.1 as i64) * (b.0 as i64 - o.0 as i64)
    };

    let mut lower = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0 {
            lower.pop();
        }
        lower.push(p);
    }

    let mut upper = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0 {
            upper.pop();
        }
        upper.push(p);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Minimum-area bounding rectangle using rotating calipers on a convex polygon.
/// Returns 4 corner points of the oriented bounding box.
fn minimum_area_bounding_rect(hull: &[(i32, i32)]) -> Quad {
    let n = hull.len();
    if n < 3 {
        let (min_x, min_y, max_x, max_y) = bounding_box(hull);
        return Quad::new(
            Point::new(min_x as f32, min_y as f32),
            Point::new(max_x as f32, min_y as f32),
            Point::new(max_x as f32, max_y as f32),
            Point::new(min_x as f32, max_y as f32),
        );
    }

    // Convert to f64 for precision
    let hull_f64: Vec<(f64, f64)> = hull.iter().map(|&(x, y)| (x as f64, y as f64)).collect();

    // Use edge directions as candidate orientations
    let mut min_area = f64::MAX;
    let mut best_rect_points: Vec<(f64, f64)> = vec![(0.0, 0.0); 4];

    for i in 0..n {
        let j = (i + 1) % n;
        let edge_x = hull_f64[j].0 - hull_f64[i].0;
        let edge_y = hull_f64[j].1 - hull_f64[i].1;
        let edge_len = (edge_x * edge_x + edge_y * edge_y).sqrt();
        if edge_len < 1e-10 {
            continue;
        }

        // Unit edge direction
        let ux = edge_x / edge_len;
        let uy = edge_y / edge_len;

        // Perpendicular direction (rotate 90 degrees CCW)
        let vx = -uy;
        let vy = ux;

        // Project all hull points onto this basis
        let mut min_proj_u = f64::MAX;
        let mut max_proj_u = f64::MIN;
        let mut min_proj_v = f64::MAX;
        let mut max_proj_v = f64::MIN;

        for &(px, py) in &hull_f64 {
            let proj_u = px * ux + py * uy;
            let proj_v = px * vx + py * vy;
            min_proj_u = min_proj_u.min(proj_u);
            max_proj_u = max_proj_u.max(proj_u);
            min_proj_v = min_proj_v.min(proj_v);
            max_proj_v = max_proj_v.max(proj_v);
        }

        let area = (max_proj_u - min_proj_u) * (max_proj_v - min_proj_v);
        if area < min_area {
            min_area = area;

            // Compute the 4 corners in world coordinates
            let corners = [
                (min_proj_u * ux + min_proj_v * vx, min_proj_u * uy + min_proj_v * vy),
                (max_proj_u * ux + min_proj_v * vx, max_proj_u * uy + min_proj_v * vy),
                (max_proj_u * ux + max_proj_v * vx, max_proj_u * uy + max_proj_v * vy),
                (min_proj_u * ux + max_proj_v * vx, min_proj_u * uy + max_proj_v * vy),
            ];
            best_rect_points = corners.to_vec();
        }
    }

    Quad::new(
        Point::new(best_rect_points[0].0 as f32, best_rect_points[0].1 as f32),
        Point::new(best_rect_points[1].0 as f32, best_rect_points[1].1 as f32),
        Point::new(best_rect_points[2].0 as f32, best_rect_points[2].1 as f32),
        Point::new(best_rect_points[3].0 as f32, best_rect_points[3].1 as f32),
    )
    .sorted()
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

/// Compute the average probability score within a quadrilateral region on the probability map.
/// Uses the quad's bounding rect on the scaled-down probability map for mask scoring.
fn polygon_average_score(
    map: &[f32],
    width: usize,
    height: usize,
    quad: &Quad,
    scale: f32,
) -> f32 {
    // Scale quad coordinates down to probability map coordinate space
    let scaled = quad.scale(1.0 / scale, 1.0 / scale);
    let brect = scaled.bounding_rect();

    let x1 = brect.x.max(0.0) as usize;
    let y1 = brect.y.max(0.0) as usize;
    let x2 = (brect.right() as usize).min(width);
    let y2 = (brect.bottom() as usize).min(height);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_det_params_from_paddle_fallback() {
        // Simulate config from from_paddle_inference_dir() — PP-OCRv6 without config.json
        let config = latexsnipper_model::ModelConfig {
            model_type: "crnn_ctc".into(),
            model_family: Some("PaddleOCRv6 Text Recognition".into()),
            input: Some(latexsnipper_model::InputConfig {
                name: "x".into(),
                shape: vec![1, 3, 48, 3200],
                dtype: "float32".into(),
                range: Some(vec![0.0, 1.0]),
            }),
            output: Some(latexsnipper_model::OutputConfig {
                name: "fetch_name_0".into(),
                shape: vec![1, -1, -1],
                description: None,
            }),
            preprocessing: Some(latexsnipper_model::PreprocessConfig {
                color_format: Some("BGR".into()),
                normalization: Some(latexsnipper_model::NormalizationConfig {
                    mean: Some(vec![0.5, 0.5, 0.5]),
                    std: Some(vec![0.5, 0.5, 0.5]),
                }),
                resize: None,
                divisible_by: None,
                pad_value: None,
            }),
            postprocessing: None,
            decoding: None,
            dynamic_shapes: Some(true),
            // Remaining fields: explicit None
            license: None,
            task_type: None,
            num_classes: None,
            encoder: None,
            decoder: None,
            quantization: None,
            outputs: None,
            extra: None,
            pipeline: None,
        };

        let params = TextDetParams::from_config(&config);

        assert_eq!(params.max_side, 960, "should keep default when config lacks postprocessing");
        assert_eq!(params.input_name, "x");
        assert_eq!(params.color_format, "bgr");
        assert_eq!(params.mean, [0.5, 0.5, 0.5]);
        assert_eq!(params.std, [0.5, 0.5, 0.5]);
        assert_eq!(params.det_threshold, 0.3);
        assert_eq!(params.box_threshold, 0.5);
        assert_eq!(params.unclip_ratio, 1.6);
    }

    #[test]
    fn test_text_det_params_from_dbnet_config() {
        let config = latexsnipper_model::ModelConfig {
            model_type: "dbnet".into(),
            model_family: Some("Test DBNet".into()),
            input: Some(latexsnipper_model::InputConfig {
                name: "input".into(),
                shape: vec![1, 3, 640, 640],
                dtype: "float32".into(),
                range: None,
            }),
            output: Some(latexsnipper_model::OutputConfig {
                name: "output".into(),
                shape: vec![1, 1, 160, 160],
                description: None,
            }),
            preprocessing: Some(latexsnipper_model::PreprocessConfig {
                color_format: Some("RGB".into()),
                normalization: Some(latexsnipper_model::NormalizationConfig {
                    mean: Some(vec![0.0, 0.0, 0.0]),
                    std: Some(vec![1.0, 1.0, 1.0]),
                }),
                resize: None,
                divisible_by: Some(32),
                pad_value: None,
            }),
            postprocessing: Some(latexsnipper_model::PostprocessConfig {
                postprocess_type: None,
                confidence_threshold: None,
                iou_threshold: None,
                threshold: Some(0.2),
                box_threshold: Some(0.6),
                max_detections: None,
                max_candidates: Some(500),
                unclip_ratio: Some(2.0),
                box_type: Some(latexsnipper_model::DbNetBoxType::Quad),
                score_mode: Some(latexsnipper_model::DbNetScoreMode::Fast),
                anchors: None,
                strides: None,
                reg_max: None,
                num_queries: None,
                apply_sigmoid: None,
                output_layout: None,
                extra: None,
            }),
            decoding: None,
            // Remaining fields: explicit None
            dynamic_shapes: None,
            license: None,
            task_type: None,
            num_classes: None,
            encoder: None,
            decoder: None,
            quantization: None,
            outputs: None,
            extra: None,
            pipeline: None,
        };

        let params = TextDetParams::from_config(&config);

        assert_eq!(params.input_name, "input");
        assert_eq!(params.max_side, 640);
        assert_eq!(params.stride, 32);
        assert_eq!(params.det_threshold, 0.2);
        assert_eq!(params.box_threshold, 0.6);
        assert_eq!(params.unclip_ratio, 2.0);
        assert_eq!(params.max_candidates, 500);
        assert_eq!(params.box_type, "quad");
        assert_eq!(params.score_mode, "fast");
        assert_eq!(params.mean, [0.0, 0.0, 0.0]);
        assert_eq!(params.std, [1.0, 1.0, 1.0]);
    }
}
