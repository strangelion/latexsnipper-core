use latexsnipper_ast::{Block, LayoutPosition, ReadingOrderRole, Rect};

/// Reading order logic for sorting blocks in a document.
pub struct ReadingOrder;

impl ReadingOrder {
    /// Sort blocks into reading order using y-bucket + x tie-breaker.
    ///
    /// Blocks within the same y-bucket (within threshold) are sorted by x.
    /// Blocks in different y-buckets are sorted by y-center.
    pub fn sort(blocks: &mut [Block], y_threshold: f32) {
        if blocks.len() <= 1 {
            return;
        }

        // Extract geometry info for sorting
        let mut indexed: Vec<(usize, f32, f32)> = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let geom = b.geometry();
                let y = geom.map_or(0.0, |g| g.y + g.height / 2.0);
                let x = geom.map_or(0.0, |g| g.x);
                (i, x, y)
            })
            .collect();

        // Sort by y-center first
        indexed.sort_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });

        // Group into y-buckets
        let mut buckets: Vec<Vec<(usize, f32, f32)>> = Vec::new();

        for item in indexed {
            if let Some(last_bucket) = buckets.last_mut() {
                let last_y = last_bucket.first().map_or(0.0, |i| i.2);
                if (item.2 - last_y).abs() < y_threshold {
                    last_bucket.push(item);
                    continue;
                }
            }
            buckets.push(vec![item]);
        }

        // Sort within each bucket by x
        for bucket in &mut buckets {
            bucket.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        }

        // Flatten and reorder blocks
        let order: Vec<usize> = buckets.into_iter().flatten().map(|(i, _, _)| i).collect();
        let original = blocks.to_vec();
        for (new_idx, old_idx) in order.into_iter().enumerate() {
            blocks[new_idx] = original[old_idx].clone();
        }
    }
}

/// Column detection result for a page.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBounds {
    /// Column x-ranges in page coordinates, left to right.
    pub columns: Vec<(f32, f32)>,
}

/// Analyze a page's block geometry and detect column structure.
///
/// Column detection uses stable vertical whitespace gutters across the page
/// content area. A column gutter must be wider than `min_gutter` and span
/// most of the content height. When no gutter is found a single column is
/// returned (the common case, including narrow-gutter single-column pages).
pub fn detect_columns(blocks: &[Block], page_width: f32, min_gutter: f32) -> ColumnBounds {
    if blocks.is_empty() {
        return ColumnBounds {
            columns: vec![(0.0, page_width)],
        };
    }

    let geoms: Vec<Rect> = blocks
        .iter()
        .filter_map(|b| b.geometry().copied())
        .collect();
    if geoms.is_empty() {
        return ColumnBounds {
            columns: vec![(0.0, page_width)],
        };
    }

    let min_x = geoms.iter().map(|g| g.x).fold(f32::MAX, f32::min).max(0.0);
    let max_x = geoms
        .iter()
        .map(|g| g.right())
        .fold(0.0, f32::max)
        .min(page_width);
    let top = geoms.iter().map(|g| g.y).fold(f32::MAX, f32::min);
    let bottom = geoms.iter().map(|g| g.bottom()).fold(0.0, f32::max);
    let content_height = (bottom - top).max(1.0);

    // Sample the x-axis for vertical whitespace runs wide enough to be a
    // real gutter. A gutter must also span most of the content height: a
    // wide gap that only cuts a few lines (e.g. between a heading and the
    // body) is an intra-line gap, not a column gutter.
    let sample_step = 4.0f32;
    let mut x = min_x;
    let mut gaps: Vec<(f32, f32)> = Vec::new();
    let mut gap_start: Option<f32> = None;

    while x < max_x {
        let covered = geoms.iter().any(|g| x >= g.x && x < g.right());
        if !covered && gap_start.is_none() {
            gap_start = Some(x);
        } else if covered {
            if let Some(start) = gap_start.take() {
                if x - start >= min_gutter && gap_span(&geoms, start, x) >= content_height * 0.5 {
                    gaps.push((start, x));
                }
            }
        }
        x += sample_step;
    }
    if let Some(start) = gap_start.take() {
        if max_x - start >= min_gutter && gap_span(&geoms, start, max_x) >= content_height * 0.5 {
            gaps.push((start, max_x));
        }
    }

    if gaps.is_empty() {
        return ColumnBounds {
            columns: vec![(min_x, max_x)],
        };
    }

    // Build column ranges from the gaps.
    let mut columns = Vec::new();
    let mut cursor = min_x;
    for (gs, ge) in &gaps {
        columns.push((cursor, *gs));
        cursor = *ge;
    }
    columns.push((cursor, max_x));
    // Keep only columns with nonzero width.
    columns.retain(|(a, b)| b - a > 1.0);

    ColumnBounds { columns }
}

/// Vertical span covered by blocks on both sides of the gap. A real column
/// gutter is flanked by content (left column + right column) spanning most
/// of the content height; a gap cutting a single line has a shallow span.
fn gap_span(geoms: &[Rect], gap_start: f32, gap_end: f32) -> f32 {
    let mut min_y = f32::MAX;
    let mut max_y = 0.0f32;
    let mut found = false;
    for g in geoms {
        // The block is entirely left of the gap or entirely right of it —
        // either way it flanks the gutter.
        let on_left = g.right() <= gap_start;
        let on_right = g.x >= gap_end;
        if (on_left || on_right) && g.width > 1.0 {
            min_y = min_y.min(g.y);
            max_y = max_y.max(g.bottom());
            found = true;
        }
    }
    if !found {
        0.0
    } else {
        (max_y - min_y).max(0.0)
    }
}

/// Assign structured layout positions to every block on a page.
///
/// For each block with geometry it computes:
/// - `columnId` from [`detect_columns`];
/// - `lineId` from y-buckets within each column;
/// - `paragraphId` from vertical gaps between lines;
/// - `readingOrder` column-major (left column top-to-bottom, then next);
/// - `role` (heading/paragraph/header/footer/pageNumber/caption/reference/
///   formulaLeadIn) from geometry heuristics;
/// - `isDisplayFormula` from the block kind.
///
/// Blocks without geometry are left untouched. Y-bucketing is only a
/// fallback when no explicit column geometry is available.
pub fn assign_layout_positions(
    blocks: &mut [Block],
    page_width: f32,
    page_height: f32,
    page_index: usize,
) {
    // Detect columns from geometry. The gutter threshold scales with the
    // page (a 40px gutter on a 800px page is a real column gap; on a
    // 2000px page it is not).
    let gutter = (page_width * 0.03).clamp(24.0, 120.0);
    let bounds = detect_columns(blocks, page_width, gutter);

    // Assign column per block.
    let mut column_lines: Vec<Vec<usize>> = vec![Vec::new(); bounds.columns.len()];

    for (idx, block) in blocks.iter().enumerate() {
        let Some(geom) = block.geometry() else {
            continue;
        };
        let center_x = geom.x + geom.width / 2.0;
        let col = bounds
            .columns
            .iter()
            .position(|(a, b)| center_x >= *a && center_x <= *b)
            .unwrap_or(0);
        column_lines[col].push(idx);
    }

    // Within each column, bucket lines by y and paragraphs by vertical gap.
    let mut global_order = 0usize;
    let mut roles: Vec<ReadingOrderRole> = vec![ReadingOrderRole::Unknown; blocks.len()];
    let mut display: Vec<bool> = vec![false; blocks.len()];
    let mut line_ids: Vec<usize> = vec![0; blocks.len()];
    let mut paragraph_ids: Vec<usize> = vec![0; blocks.len()];
    let mut column_ids: Vec<usize> = vec![0; blocks.len()];

    for (col, col_indices) in column_lines.iter_mut().enumerate() {
        // Sort by y then x within the column.
        col_indices.sort_by(|&a, &b| {
            let (ga, gb) = (blocks[a].geometry().unwrap(), blocks[b].geometry().unwrap());
            ga.y.partial_cmp(&gb.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ga.x.partial_cmp(&gb.x).unwrap_or(std::cmp::Ordering::Equal))
        });

        // Line/paragraph thresholds scale with the page so the same layout
        // produces the same structure at any DPI.
        let y_threshold = (page_height * 0.012).max(8.0);
        let paragraph_gap = (page_height * 0.02).max(14.0);
        let mut line_id = 0usize;
        let mut paragraph_id = 0usize;
        let mut prev_y: Option<f32> = None;
        let mut prev_bottom: Option<f32> = None;

        for &idx in col_indices.iter() {
            let geom = blocks[idx].geometry().unwrap();
            let center_y = geom.y + geom.height / 2.0;
            if let Some(py) = prev_y {
                if (center_y - py).abs() >= y_threshold {
                    line_id += 1;
                }
            }
            if let Some(pb) = prev_bottom {
                if geom.y - pb >= paragraph_gap {
                    paragraph_id += 1;
                }
            }
            line_ids[idx] = line_id;
            paragraph_ids[idx] = paragraph_id;
            column_ids[idx] = col;
            roles[idx] = infer_role(&blocks[idx], geom, page_height);
            display[idx] = matches!(blocks[idx], Block::Formula(_));
            prev_y = Some(center_y);
            prev_bottom = Some(geom.bottom());
        }
    }

    // Assign global reading order: column-major.
    for col_indices in &column_lines {
        for &idx in col_indices {
            let pos = LayoutPosition {
                page_index,
                column_id: column_ids[idx],
                line_id: line_ids[idx],
                paragraph_id: paragraph_ids[idx],
                reading_order: global_order,
                role: roles[idx],
                is_display_formula: display[idx],
            };
            if let Some(source) = blocks[idx].source_mut() {
                *source = source.clone().with_layout_position(pos);
            }
            global_order += 1;
        }
    }
}

/// Infer the reading-order role of a block from geometry heuristics.
///
/// Order matters: page numbers (small, right-aligned in the footer band)
/// are detected before generic footers so a footer-page number is not
/// misclassified. A display formula is a paragraph-level content block, not
/// a lead-in; `FormulaLeadIn` is only for short text immediately preceding
/// a display formula (detected by the caller when applicable).
fn infer_role(block: &Block, geom: &Rect, page_height: f32) -> ReadingOrderRole {
    if matches!(block, Block::Heading(_)) {
        return ReadingOrderRole::Heading;
    }
    // Header band (top 6%).
    if geom.y < page_height * 0.06 {
        return ReadingOrderRole::Header;
    }
    // Footer band (bottom 6%): a small isolated box there is a page number.
    if geom.bottom() > page_height * 0.94 {
        let normalized_height = geom.height / page_height.max(1.0);
        let normalized_width = geom.width / page_height.max(1.0);
        let looks_like_page_number = normalized_height <= 0.03 && normalized_width <= 0.1;
        if looks_like_page_number {
            return ReadingOrderRole::PageNumber;
        }
        return ReadingOrderRole::Footer;
    }
    if matches!(block, Block::Figure(_)) {
        return ReadingOrderRole::Caption;
    }
    if matches!(block, Block::Formula(_)) {
        return ReadingOrderRole::Paragraph;
    }
    ReadingOrderRole::Paragraph
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::{Formula, FormulaBlock, Inline, ParagraphBlock, SourceInfo, TextRun};

    fn make_text_block(text: &str, x: f32, y: f32) -> Block {
        Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun::new(text))],
            geometry: Some(Rect::new(x, y, 100.0, 20.0)),
            source: Some(SourceInfo::new()),
            style: None,
        })
    }

    fn make_formula_block(latex: &str, x: f32, y: f32) -> Block {
        Block::Formula(FormulaBlock {
            formula: Formula::latex(latex),
            label: None,
            number: None,
            environment: None,
            geometry: Some(Rect::new(x, y, 50.0, 20.0)),
            source: Some(SourceInfo::new()),
        })
    }

    #[test]
    fn test_sort_single_block() {
        let mut blocks = vec![make_text_block("a", 10.0, 10.0)];
        ReadingOrder::sort(&mut blocks, 5.0);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn test_sort_same_line() {
        let mut blocks = vec![
            make_text_block("b", 200.0, 10.0),
            make_text_block("a", 10.0, 10.0),
        ];
        ReadingOrder::sort(&mut blocks, 5.0);
        // Same y, sorted by x
        assert_eq!(blocks[0].geometry().unwrap().x, 10.0);
        assert_eq!(blocks[1].geometry().unwrap().x, 200.0);
    }

    #[test]
    fn test_sort_different_lines() {
        let mut blocks = vec![
            make_text_block("bottom", 10.0, 100.0),
            make_text_block("top", 10.0, 10.0),
        ];
        ReadingOrder::sort(&mut blocks, 5.0);
        // Different y, sorted by y
        assert_eq!(blocks[0].geometry().unwrap().y, 10.0);
        assert_eq!(blocks[1].geometry().unwrap().y, 100.0);
    }

    #[test]
    fn test_sort_mixed() {
        let mut blocks = vec![
            make_formula_block("b", 200.0, 10.0),
            make_text_block("a", 10.0, 10.0),
            make_text_block("c", 10.0, 100.0),
        ];
        ReadingOrder::sort(&mut blocks, 5.0);
        // Line 1: a(x=10), b(x=200)
        // Line 2: c(y=100)
        assert_eq!(blocks[0].geometry().unwrap().x, 10.0);
        assert_eq!(blocks[1].geometry().unwrap().x, 200.0);
        assert_eq!(blocks[2].geometry().unwrap().y, 100.0);
    }

    #[test]
    fn test_detect_columns_single() {
        let blocks = vec![
            make_text_block("a", 10.0, 10.0),
            make_text_block("b", 10.0, 200.0),
        ];
        let bounds = detect_columns(&blocks, 600.0, 40.0);
        assert_eq!(bounds.columns.len(), 1);
    }

    #[test]
    fn test_detect_columns_two_column() {
        // Two text columns separated by a wide gutter.
        let mut blocks = Vec::new();
        for y in [10.0, 100.0, 190.0] {
            blocks.push(make_text_block("l", 20.0, y));
            blocks.push(make_text_block("r", 320.0, y));
        }
        let bounds = detect_columns(&blocks, 600.0, 40.0);
        assert_eq!(bounds.columns.len(), 2);
        let (a, _) = bounds.columns[0];
        let (b, _) = bounds.columns[1];
        assert!(a < 300.0);
        assert!(b > 300.0);
    }

    #[test]
    fn test_assign_layout_positions_single_column() {
        let mut blocks = vec![
            make_text_block("first", 10.0, 10.0),
            make_text_block("second", 10.0, 100.0),
            make_formula_block("f", 10.0, 180.0),
        ];
        assign_layout_positions(&mut blocks, 600.0, 800.0, 0);

        let p0 = blocks[0].source().unwrap().layout_position.unwrap();
        let p1 = blocks[1].source().unwrap().layout_position.unwrap();
        let pf = blocks[2].source().unwrap().layout_position.unwrap();

        assert_eq!(p0.column_id, 0);
        assert_eq!(p0.reading_order, 0);
        assert_eq!(p1.reading_order, 1);
        assert_eq!(pf.reading_order, 2);
        assert!(pf.is_display_formula);
        // A display formula is paragraph-level content, not a lead-in.
        assert_eq!(pf.role, ReadingOrderRole::Paragraph);
        // Different lines.
        assert_ne!(p0.line_id, p1.line_id);
    }

    #[test]
    fn test_assign_layout_positions_two_column_order() {
        // Left column blocks, then right column blocks.
        let mut blocks = vec![
            make_text_block("l1", 20.0, 10.0),
            make_text_block("r1", 320.0, 10.0),
            make_text_block("l2", 20.0, 100.0),
            make_text_block("r2", 320.0, 100.0),
        ];
        assign_layout_positions(&mut blocks, 600.0, 800.0, 0);

        let orders: Vec<(usize, usize)> = blocks
            .iter()
            .map(|b| {
                let p = b.source().unwrap().layout_position.unwrap();
                (p.reading_order, p.column_id)
            })
            .collect();
        // Left column first (orders 0,1), then right column (orders 2,3).
        let l_orders: Vec<usize> = orders
            .iter()
            .filter(|(_, c)| *c == 0)
            .map(|(o, _)| *o)
            .collect();
        let r_orders: Vec<usize> = orders
            .iter()
            .filter(|(_, c)| *c == 1)
            .map(|(o, _)| *o)
            .collect();
        assert_eq!(l_orders, vec![0, 1]);
        assert_eq!(r_orders, vec![2, 3]);
    }

    #[test]
    fn test_header_footer_roles() {
        let mut blocks = vec![
            make_text_block("header", 10.0, 5.0),
            make_text_block("body", 10.0, 100.0),
            make_text_block("page", 10.0, 790.0),
        ];
        assign_layout_positions(&mut blocks, 600.0, 800.0, 0);
        let roles: Vec<ReadingOrderRole> = blocks
            .iter()
            .map(|b| b.source().unwrap().layout_position.unwrap().role)
            .collect();
        assert_eq!(roles[0], ReadingOrderRole::Header);
        assert_eq!(roles[1], ReadingOrderRole::Paragraph);
        assert!(matches!(
            roles[2],
            ReadingOrderRole::PageNumber | ReadingOrderRole::Footer
        ));
    }
}
