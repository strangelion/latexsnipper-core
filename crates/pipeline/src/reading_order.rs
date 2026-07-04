use latexsnipper_ast::Block;

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

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::{Formula, FormulaBlock, Inline, ParagraphBlock, Rect, SourceInfo, TextRun};

    fn make_text_block(text: &str, x: f32, y: f32) -> Block {
        Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun::new(text))],
            geometry: Some(Rect::new(x, y, 100.0, 20.0)),
            source: Some(SourceInfo::new()),
        })
    }

    fn make_formula_block(latex: &str, x: f32, y: f32) -> Block {
        Block::Formula(FormulaBlock {
            formula: Formula::latex(latex),
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
}
