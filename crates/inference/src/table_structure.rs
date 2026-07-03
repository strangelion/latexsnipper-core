use crate::types::GridCell;
use latexsnipper_ast::Rect;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

/// Two-layer table recognition architecture:
///
/// ```text
/// Page Image
///     ↓
/// [Layer 1: Table Detection] -- table-det model (PicoDet/TATR)
///     ↓
/// Table Region (cropped)
///     ↓
/// [Layer 2: Structure Recognition] -- table-struct model (SLANet/TATR)
///     ↓
/// Cell coordinates + row/col indices + merge info
///     ↓
/// [OCR] -- text-det + text-rec per cell
///     ↓
/// TableBlock with TableCell content
/// ```
///
/// Recommended model combinations:
/// - Chinese Office docs: PP-Structure layout + SLANet_plus
/// - Academic PDFs: TATR detection + TATR structure
///   - MVP / lightweight: YOLO table det (detection only, no structure)
///
/// Information about a row in the table.
#[derive(Debug, Clone)]
pub struct RowInfo {
    pub y_start: f32,
    pub y_end: f32,
    pub height: f32,
}

/// Information about a column in the table.
#[derive(Debug, Clone)]
pub struct ColInfo {
    pub x_start: f32,
    pub x_end: f32,
    pub width: f32,
}

/// Information about a cell in the table.
#[derive(Debug, Clone)]
pub struct CellInfo {
    pub row: usize,
    pub col: usize,
    pub rowspan: u32,
    pub colspan: u32,
    pub rect: Rect,
}

/// Parsed table structure from a structure recognition model.
///
/// This is what SLANet_plus or TATR structure recognition outputs:
/// - Cell bounding boxes with row/col indices
/// - Merged cell information (rowspan/colspan)
/// - Row and column boundaries
#[derive(Debug, Clone)]
pub struct TableStructure {
    pub rows: Vec<RowInfo>,
    pub cols: Vec<ColInfo>,
    pub cells: Vec<CellInfo>,
    pub rect: Rect,
}

/// Structure recognition backend.
///
/// Different models have different output formats:
/// - SLANet_plus: outputs cell bboxes + grid indices
/// - TATR: outputs DETR-style object queries
/// - Projection: fallback line-based analysis (no model needed)
pub enum TableStructBackend {
    /// PaddleOCR SLANet_plus (recommended for Chinese docs)
    SlaNetPlus,
    /// Microsoft TATR structure recognition (best for academic papers)
    TatrStructure,
    /// Fallback: projection-based line detection (no model, lower quality)
    Projection,
}

// ============================================================================
// SLANet Plus Output Decoder
// ============================================================================

/// Structure vocabulary for SLANet output decoding.
///
/// The model outputs class indices that map to HTML-like table structure tokens.
/// These tokens describe the table layout: rows, cells, headers, etc.
///
/// Source: PaddleOCR table_structure_dict_ch.txt
/// Note: With merge_no_span_structure=True (default), `<td>` is replaced by `<td></td>`.
/// The vocabulary size is 50 tokens matching the model output.
const SLANET_STRUCTURE_DICT: &[&str] = &[
    "<thead>",         // 0
    "</thead>",        // 1
    "<tbody>",         // 2
    "</tbody>",        // 3
    "<tr>",            // 4
    "</tr>",           // 5
    "<td></td>",       // 6 (merged with merge_no_span_structure)
    "<td",             // 7
    ">",               // 8
    "</td>",           // 9
    " colspan=\"2\"",  // 10
    " colspan=\"3\"",  // 11
    " colspan=\"4\"",  // 12
    " colspan=\"5\"",  // 13
    " colspan=\"6\"",  // 14
    " colspan=\"7\"",  // 15
    " colspan=\"8\"",  // 16
    " colspan=\"9\"",  // 17
    " colspan=\"10\"", // 18
    " colspan=\"11\"", // 19
    " colspan=\"12\"", // 20
    " colspan=\"13\"", // 21
    " colspan=\"14\"", // 22
    " colspan=\"15\"", // 23
    " colspan=\"16\"", // 24
    " colspan=\"17\"", // 25
    " colspan=\"18\"", // 26
    " colspan=\"19\"", // 27
    " colspan=\"20\"", // 28
    " rowspan=\"2\"",  // 29
    " rowspan=\"3\"",  // 30
    " rowspan=\"4\"",  // 31
    " rowspan=\"5\"",  // 32
    " rowspan=\"6\"",  // 33
    " rowspan=\"7\"",  // 34
    " rowspan=\"8\"",  // 35
    " rowspan=\"9\"",  // 36
    " rowspan=\"10\"", // 37
    " rowspan=\"11\"", // 38
    " rowspan=\"12\"", // 39
    " rowspan=\"13\"", // 40
    " rowspan=\"14\"", // 41
    " rowspan=\"15\"", // 42
    " rowspan=\"16\"", // 43
    " rowspan=\"17\"", // 44
    " rowspan=\"18\"", // 45
    " rowspan=\"19\"", // 46
    " rowspan=\"20\"", // 47
    "sos",             // 48 (start of sequence)
    "eos",             // 49 (end of sequence)
];

/// Preprocess image for SLANet inference.
///
/// Resizes to max 488px, normalizes with ImageNet stats, pads to 488x488.
pub fn preprocess_for_slanet(image: &SnipperImage) -> Result<(Vec<f32>, [f32; 4])> {
    let max_len = 488.0f32;
    let w = image.width() as f32;
    let h = image.height() as f32;

    // Calculate resize ratio
    let ratio = max_len / w.max(h);
    let resize_w = (w * ratio + 0.5) as usize;
    let resize_h = (h * ratio + 0.5) as usize;

    // Resize image
    let resized = latexsnipper_image::operations::resize(image, resize_w as u32, resize_h as u32);

    // Normalize with ImageNet stats
    let mean = [0.485, 0.456, 0.406];
    let std = [0.229, 0.224, 0.225];
    let normalized = latexsnipper_image::operations::normalize(&resized, &mean, &std);

    // Pad to 488x488
    let mut padded = vec![0.0f32; 3 * 488 * 488];
    let pixels = normalized;
    let channels = 3;

    for c in 0..channels {
        for y in 0..resize_h {
            for x in 0..resize_w {
                let src_idx = (y * resize_w + x) * channels + c;
                let dst_idx = c * 488 * 488 + y * 488 + x;
                if src_idx < pixels.len() && dst_idx < padded.len() {
                    padded[dst_idx] = pixels[src_idx];
                }
            }
        }
    }

    // Shape info for bbox decoding: [resized_h, resized_w, orig_h, orig_w]
    // Python uses: h, w = shape[:2] then bbox[0::2] *= w, bbox[1::2] *= h
    // So shape[:2] should be the RESIZED dimensions for bbox scaling
    let shape_info = [resize_h as f32, resize_w as f32, h, w];

    Ok((padded, shape_info))
}

/// Run SLANet_plus inference and return a list of GridCell.
///
/// SLANet_plus has two outputs:
/// - cell_coords: [1, max_cells, 8] — quadrilateral per cell (normalized)
/// - structure_logits: [1, max_cells, vocab_size] — structure token logits
///
/// The structure tokens form an HTML-like stream (<tr>, <td>, etc.) that
/// determines the row/col indices and merge tags for each cell.
pub fn recognize_structure_slanet(
    image: &SnipperImage,
    session: &dyn InferenceSession,
) -> Result<Vec<GridCell>> {
    let orig_w = image.width() as f32;
    let orig_h = image.height() as f32;

    let (padded, shape_info) = preprocess_for_slanet(image)?;

    let input = Tensor::float32("x", vec![1, 3, 488, 488], padded);

    let outputs = session.run(&[input])?;
    if outputs.len() < 2 {
        return Err(SnipperError::Inference("SLANet expected 2 outputs".into()));
    }

    let cell_coords = outputs[0]
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("cell_coords not float32".into()))?;
    let structure_logits = outputs[1]
        .as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("structure_logits not float32".into()))?;

    let structure =
        decode_slanet_output(cell_coords, structure_logits, &shape_info, orig_w, orig_h)?;

    // Convert TableStructure -> Vec<GridCell>
    let grid_cells: Vec<GridCell> = structure
        .cells
        .iter()
        .map(|c| GridCell {
            row: c.row,
            col: c.col,
            rowspan: c.rowspan,
            colspan: c.colspan,
            rect: c.rect,
        })
        .collect();

    Ok(grid_cells)
}

/// Unified table structure recognition entry point.
///
/// Supported backends: "tatr", "slanet", "projection".
/// If the model file is not found under `models/table-struct/{backend}/`, returns None.
pub fn recognize_table_structure(
    image: &SnipperImage,
    backend: &str,
    backend_session: Option<&dyn InferenceSession>,
) -> Result<Option<Vec<GridCell>>> {
    match backend {
        "tatr" => {
            if let Some(session) = backend_session {
                let dets = crate::table_transformer::recognize_table_transformer(image, session)?;
                let cells = crate::table_transformer::build_grid_from_detections(
                    &dets,
                    image.width() as f32,
                    image.height() as f32,
                );
                Ok(Some(cells))
            } else {
                Ok(None)
            }
        }
        "slanet" => {
            if let Some(session) = backend_session {
                let cells = recognize_structure_slanet(image, session)?;
                Ok(Some(cells))
            } else {
                Ok(None)
            }
        }
        "projection" => {
            let rect = Rect::new(0.0, 0.0, image.width() as f32, image.height() as f32);
            let structure = parse_table_structure(image, &rect)?;
            let cells: Vec<GridCell> = structure
                .cells
                .iter()
                .map(|c| GridCell {
                    row: c.row,
                    col: c.col,
                    rowspan: c.rowspan,
                    colspan: c.colspan,
                    rect: c.rect,
                })
                .collect();
            Ok(Some(cells))
        }
        _ => Err(SnipperError::Inference(format!(
            "Unknown table structure backend: {}",
            backend
        ))),
    }
}

/// Decode SLANet model outputs into table structure.
///
/// # Arguments
/// * `cell_coords` - First output: [1, max_cells, 8] bbox predictions (normalized [0,1])
/// * `structure_logits` - Second output: [1, max_cells, vocab_size] class logits
/// * `shape_info` - [resized_h, resized_w, orig_h, orig_w] from preprocessing
/// * `img_width` - Original image width (for final rect)
/// * `img_height` - Original image height (for final rect)
pub fn decode_slanet_output(
    cell_coords: &[f32],
    structure_logits: &[f32],
    shape_info: &[f32; 4],
    img_width: f32,
    img_height: f32,
) -> Result<TableStructure> {
    // Python uses: h, w = shape[:2] then bbox[0::2] *= w, bbox[1::2] *= h
    // So we need RESIZED dimensions for bbox scaling
    let resized_h = shape_info[0];
    let resized_w = shape_info[1];

    // Get argmax of structure logits for each position
    let num_positions = structure_logits.len() / SLANET_STRUCTURE_DICT.len();
    let vocab_size = SLANET_STRUCTURE_DICT.len();

    let mut structure_tokens = Vec::new();
    let mut cell_bboxes = Vec::new();

    for pos in 0..num_positions {
        let logits_start = pos * vocab_size;
        let logits_end = logits_start + vocab_size;
        if logits_end > structure_logits.len() {
            break;
        }

        // Find argmax
        let (max_idx, max_val) = structure_logits[logits_start..logits_end]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap_or((0, &0.0));

        if max_idx >= SLANET_STRUCTURE_DICT.len() {
            continue;
        }

        let token = SLANET_STRUCTURE_DICT[max_idx];
        structure_tokens.push((token, *max_val));

        // If token is a cell tag, extract bbox
        // With merge_no_span_structure=True, the token is "<td></td>"
        if token == "<td></td>" || token == "<td>" || token == "<td" {
            let coords_start = pos * 8;
            let coords_end = coords_start + 8;
            if coords_end <= cell_coords.len() {
                let bbox = &cell_coords[coords_start..coords_end];

                // Decode bbox: multiply by resized dimensions (Python: bbox[0::2] *= w, bbox[1::2] *= h)
                let mut decoded = [0.0f32; 8];
                for i in 0..4 {
                    decoded[i * 2] = bbox[i * 2] * resized_w;
                    decoded[i * 2 + 1] = bbox[i * 2 + 1] * resized_h;
                }

                // Calculate bounding rect from quadrilateral
                let x_min = decoded[0].min(decoded[2]).min(decoded[4]).min(decoded[6]);
                let y_min = decoded[1].min(decoded[3]).min(decoded[5]).min(decoded[7]);
                let x_max = decoded[0].max(decoded[2]).max(decoded[4]).max(decoded[6]);
                let y_max = decoded[1].max(decoded[3]).max(decoded[5]).max(decoded[7]);

                cell_bboxes.push(Rect::new(x_min, y_min, x_max - x_min, y_max - y_min));
            }
        }
    }

    // Parse structure tokens to extract row/col info
    let cells = parse_structure_to_cells(&structure_tokens, &cell_bboxes);

    // Build row/col info from cells
    let rows = build_row_info(&cells);
    let cols = build_col_info(&cells);

    Ok(TableStructure {
        rows,
        cols,
        cells,
        rect: Rect::new(0.0, 0.0, img_width, img_height),
    })
}

/// Parse structure tokens and cell bboxes into CellInfo list.
fn parse_structure_to_cells(tokens: &[(&str, f32)], bboxes: &[Rect]) -> Vec<CellInfo> {
    let mut cells = Vec::new();
    let mut current_row = 0;
    let mut current_col = 0;
    let mut bbox_idx = 0;

    for (token, _score) in tokens {
        match *token {
            "<tr>" => {
                current_col = 0;
            }
            "</tr>" => {
                current_row += 1;
            }
            "<td>" | "<td" | "<td></td>" => {
                if bbox_idx < bboxes.len() {
                    let rect = bboxes[bbox_idx];
                    bbox_idx += 1;

                    cells.push(CellInfo {
                        row: current_row,
                        col: current_col,
                        rowspan: 1,
                        colspan: 1,
                        rect,
                    });
                }
                current_col += 1;
            }
            " rowspan=" => {
                // Next token should be the rowspan value
                // For now, default to 1
            }
            " colspan=" => {
                // Next token should be the colspan value
                // For now, default to 1
            }
            _ => {}
        }
    }

    cells
}

/// Build row information from cells.
fn build_row_info(cells: &[CellInfo]) -> Vec<RowInfo> {
    if cells.is_empty() {
        return Vec::new();
    }

    let mut row_map: std::collections::HashMap<usize, Vec<f32>> = std::collections::HashMap::new();

    for cell in cells {
        row_map.entry(cell.row).or_default().push(cell.rect.y);
        row_map
            .entry(cell.row)
            .or_default()
            .push(cell.rect.y + cell.rect.height);
    }

    let mut rows: Vec<RowInfo> = row_map
        .iter()
        .map(|(&_row_idx, y_vals)| {
            let y_min = y_vals.iter().cloned().fold(f32::INFINITY, f32::min);
            let y_max = y_vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            RowInfo {
                y_start: y_min,
                y_end: y_max,
                height: y_max - y_min,
            }
        })
        .collect();

    rows.sort_by(|a, b| a.y_start.partial_cmp(&b.y_start).unwrap());
    rows
}

/// Build column information from cells.
fn build_col_info(cells: &[CellInfo]) -> Vec<ColInfo> {
    if cells.is_empty() {
        return Vec::new();
    }

    let mut col_map: std::collections::HashMap<usize, Vec<f32>> = std::collections::HashMap::new();

    for cell in cells {
        col_map.entry(cell.col).or_default().push(cell.rect.x);
        col_map
            .entry(cell.col)
            .or_default()
            .push(cell.rect.x + cell.rect.width);
    }

    let mut cols: Vec<ColInfo> = col_map
        .iter()
        .map(|(&_col_idx, x_vals)| {
            let x_min = x_vals.iter().cloned().fold(f32::INFINITY, f32::min);
            let x_max = x_vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            ColInfo {
                x_start: x_min,
                x_end: x_max,
                width: x_max - x_min,
            }
        })
        .collect();

    cols.sort_by(|a, b| a.x_start.partial_cmp(&b.x_start).unwrap());
    cols
}

/// Parse the structure of a table from an image.
///
/// Current implementation uses projection-based line detection (Layer 2 fallback).
/// For production, use SLANet_plus or TATR structure recognition.
pub fn parse_table_structure(image: &SnipperImage, table_rect: &Rect) -> Result<TableStructure> {
    // Extract the table region from the image
    let table_image = extract_table_region(image, table_rect)?;

    // Detect horizontal and vertical lines
    let horizontal_lines = detect_horizontal_lines(&table_image);
    let vertical_lines = detect_vertical_lines(&table_image);

    // Convert line positions to row/column info
    let rows = lines_to_rows(&horizontal_lines, table_rect.y);
    let cols = lines_to_cols(&vertical_lines, table_rect.x);

    // Generate cell grid
    let cells = generate_cells(&rows, &cols);

    Ok(TableStructure {
        rows,
        cols,
        cells,
        rect: *table_rect,
    })
}

/// Extract the table region from the image.
fn extract_table_region(image: &SnipperImage, rect: &Rect) -> Result<SnipperImage> {
    let x = rect.x as u32;
    let y = rect.y as u32;
    let w = rect.width as u32;
    let h = rect.height as u32;

    // Bounds check
    let x = x.min(image.width().saturating_sub(1));
    let y = y.min(image.height().saturating_sub(1));
    let w = w.min(image.width() - x);
    let h = h.min(image.height() - y);

    if w == 0 || h == 0 {
        return Err(SnipperError::Inference("Invalid table region".into()));
    }

    Ok(latexsnipper_image::operations::crop(
        image,
        Rect::new(x as f32, y as f32, w as f32, h as f32),
    ))
}

/// Detect horizontal lines in the image using projection analysis.
fn detect_horizontal_lines(image: &SnipperImage) -> Vec<f32> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let pixels = image.pixels();

    // Convert to grayscale
    let gray = to_grayscale(pixels, w, h, image.format());

    // Calculate horizontal projection (sum of dark pixels per row)
    let mut projection = vec![0.0f32; h];
    for y in 0..h {
        let mut sum = 0.0;
        for x in 0..w {
            // Check if pixel is dark (line)
            if gray[y * w + x] < 128.0 {
                sum += 1.0;
            }
        }
        projection[y] = sum;
    }

    // Find peaks in projection (horizontal lines)
    let threshold = w as f32 * 0.3; // At least 30% of width should be dark
    let mut lines = Vec::new();

    let mut in_line = false;
    let mut line_start = 0;

    for (y, &val) in projection.iter().enumerate() {
        if val > threshold {
            if !in_line {
                line_start = y;
                in_line = true;
            }
        } else {
            if in_line {
                // Line ended
                let line_center = (line_start + y) as f32 / 2.0;
                lines.push(line_center);
                in_line = false;
            }
        }
    }

    // Handle line at end of image
    if in_line {
        let line_center = (line_start + h) as f32 / 2.0;
        lines.push(line_center);
    }

    lines
}

/// Detect vertical lines in the image using projection analysis.
fn detect_vertical_lines(image: &SnipperImage) -> Vec<f32> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let pixels = image.pixels();

    // Convert to grayscale
    let gray = to_grayscale(pixels, w, h, image.format());

    // Calculate vertical projection (sum of dark pixels per column)
    let mut projection = vec![0.0f32; w];
    for x in 0..w {
        let mut sum = 0.0;
        for y in 0..h {
            if gray[y * w + x] < 128.0 {
                sum += 1.0;
            }
        }
        projection[x] = sum;
    }

    // Find peaks in projection (vertical lines)
    let threshold = h as f32 * 0.3; // At least 30% of height should be dark
    let mut lines = Vec::new();

    let mut in_line = false;
    let mut line_start = 0;

    for (x, &val) in projection.iter().enumerate() {
        if val > threshold {
            if !in_line {
                line_start = x;
                in_line = true;
            }
        } else {
            if in_line {
                let line_center = (line_start + x) as f32 / 2.0;
                lines.push(line_center);
                in_line = false;
            }
        }
    }

    if in_line {
        let line_center = (line_start + w) as f32 / 2.0;
        lines.push(line_center);
    }

    lines
}

/// Convert horizontal line positions to row info.
fn lines_to_rows(lines: &[f32], y_offset: f32) -> Vec<RowInfo> {
    if lines.len() < 2 {
        return Vec::new();
    }

    let mut rows = Vec::new();
    for i in 0..lines.len() - 1 {
        let y_start = lines[i] + y_offset;
        let y_end = lines[i + 1] + y_offset;
        let height = y_end - y_start;

        if height > 5.0 {
            // Minimum row height
            rows.push(RowInfo {
                y_start,
                y_end,
                height,
            });
        }
    }

    rows
}

/// Convert vertical line positions to column info.
fn lines_to_cols(lines: &[f32], x_offset: f32) -> Vec<ColInfo> {
    if lines.len() < 2 {
        return Vec::new();
    }

    let mut cols = Vec::new();
    for i in 0..lines.len() - 1 {
        let x_start = lines[i] + x_offset;
        let x_end = lines[i + 1] + x_offset;
        let width = x_end - x_start;

        if width > 5.0 {
            // Minimum column width
            cols.push(ColInfo {
                x_start,
                x_end,
                width,
            });
        }
    }

    cols
}

/// Generate cell grid from rows and columns.
fn generate_cells(rows: &[RowInfo], cols: &[ColInfo]) -> Vec<CellInfo> {
    let mut cells = Vec::new();

    for (row_idx, row) in rows.iter().enumerate() {
        for (col_idx, col) in cols.iter().enumerate() {
            cells.push(CellInfo {
                row: row_idx,
                col: col_idx,
                rowspan: 1,
                colspan: 1,
                rect: Rect::new(col.x_start, row.y_start, col.width, row.height),
            });
        }
    }

    cells
}

/// Convert pixel data to grayscale.
///
/// Supports all pixel formats: Gray (passthrough), Rgb/Rgba (luminosity),
/// Bgr/Bgra (B channel first).
fn to_grayscale(pixels: &[u8], width: usize, height: usize, format: PixelFormat) -> Vec<f32> {
    let mut gray = vec![0.0f32; width * height];
    let channels = format.channels();

    if channels == 1 {
        // Already grayscale
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                gray[idx] = pixels[idx] as f32;
            }
        }
        return gray;
    }

    // Determine R/G/B channel offsets based on format
    let r_off: usize;
    let g_off: usize;
    let b_off: usize;
    match format {
        PixelFormat::Rgb | PixelFormat::Rgba => {
            r_off = 0;
            g_off = 1;
            b_off = 2;
        }
        PixelFormat::Bgr | PixelFormat::Bgra => {
            r_off = 2;
            g_off = 1;
            b_off = 0;
        }
        _ => {
            r_off = 0;
            g_off = 1;
            b_off = 2;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let base = (y * width + x) * channels;
            if base + 2 < pixels.len() {
                let r = pixels[base + r_off] as f32;
                let g = pixels[base + g_off] as f32;
                let b = pixels[base + b_off] as f32;
                gray[y * width + x] = 0.299 * r + 0.587 * g + 0.114 * b;
            }
        }
    }

    gray
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_to_rows() {
        let lines = vec![0.0, 50.0, 100.0, 150.0];
        let rows = lines_to_rows(&lines, 0.0);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].y_start, 0.0);
        assert_eq!(rows[0].y_end, 50.0);
        assert_eq!(rows[0].height, 50.0);
    }

    #[test]
    fn test_lines_to_cols() {
        let lines = vec![0.0, 100.0, 200.0];
        let cols = lines_to_cols(&lines, 0.0);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].x_start, 0.0);
        assert_eq!(cols[0].x_end, 100.0);
        assert_eq!(cols[0].width, 100.0);
    }

    #[test]
    fn test_generate_cells() {
        let rows = vec![
            RowInfo {
                y_start: 0.0,
                y_end: 50.0,
                height: 50.0,
            },
            RowInfo {
                y_start: 50.0,
                y_end: 100.0,
                height: 50.0,
            },
        ];
        let cols = vec![
            ColInfo {
                x_start: 0.0,
                x_end: 100.0,
                width: 100.0,
            },
            ColInfo {
                x_start: 100.0,
                x_end: 200.0,
                width: 100.0,
            },
        ];

        let cells = generate_cells(&rows, &cols);
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0].row, 0);
        assert_eq!(cells[0].col, 0);
        assert_eq!(cells[3].row, 1);
        assert_eq!(cells[3].col, 1);
    }
}
