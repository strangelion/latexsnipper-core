/// Text segmentation around formulas.
/// Ported from LaTeXSnipper's layout.py split_text_box_around_formulas().
use latexsnipper_ast::Rect;

/// A text segment after splitting around formulas.
#[derive(Debug, Clone)]
pub struct TextSegment {
    pub box_rect: Rect,
}

/// Split a text box around formula regions.
/// This ensures text recognizer only processes pure text areas.
pub fn split_text_box_around_formulas(
    text_box: &Rect,
    formula_boxes: &[Rect],
    min_width: f32,
) -> Vec<TextSegment> {
    if formula_boxes.is_empty() {
        return vec![TextSegment {
            box_rect: *text_box,
        }];
    }

    // Start with the full text box x-range
    let mut intervals: Vec<(f32, f32)> = vec![(text_box.x, text_box.right())];

    // Find formula boxes that overlap with this text box
    let mut relevant_formulas: Vec<&Rect> = Vec::new();
    for fb in formula_boxes {
        // Check vertical overlap
        let y_overlap = (text_box.bottom().min(fb.bottom()) - text_box.y.max(fb.y)).max(0.0);
        let min_height = text_box.height.min(fb.height);

        if y_overlap > min_height * 0.6 {
            // Check horizontal overlap or proximity
            let x_gap = if fb.x > text_box.right() {
                fb.x - text_box.right()
            } else if text_box.x > fb.right() {
                text_box.x - fb.right()
            } else {
                0.0 // Overlapping
            };

            let text_width = text_box.width;
            if x_gap < text_width * 0.5 || x_gap < 50.0 {
                relevant_formulas.push(fb);
            }
        }
    }

    // Sort formulas by x position
    relevant_formulas.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

    // Split intervals at formula boundaries
    for fb in &relevant_formulas {
        let mut new_intervals = Vec::new();
        for &(start, end) in &intervals {
            if fb.x > end || fb.right() < start {
                // No overlap
                new_intervals.push((start, end));
            } else {
                // Overlap - split
                if start < fb.x {
                    new_intervals.push((start, fb.x));
                }
                if fb.right() < end {
                    new_intervals.push((fb.right(), end));
                }
            }
        }
        intervals = new_intervals;
    }

    // Filter by minimum width and create segments
    intervals
        .into_iter()
        .filter(|(start, end)| end - start >= min_width)
        .map(|(start, end)| TextSegment {
            box_rect: Rect::new(start, text_box.y, end - start, text_box.height),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_formulas() {
        let text_box = Rect::new(0.0, 0.0, 100.0, 20.0);
        let segments = split_text_box_around_formulas(&text_box, &[], 5.0);
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn test_split_around_formula() {
        let text_box = Rect::new(0.0, 0.0, 100.0, 20.0);
        let formula = Rect::new(40.0, 5.0, 20.0, 15.0);
        let segments = split_text_box_around_formulas(&text_box, &[formula], 5.0);
        assert_eq!(segments.len(), 2);
    }
}
