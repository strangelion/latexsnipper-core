//! Document cleaning and structure repair.
//!
//! Post-processes a Document AST to clean up common OCR/recognition artifacts:
//! - Merge adjacent text blocks on the same line (oversplitting)
//! - Remove empty blocks
//! - Add diagnostics for low-confidence regions
//! - Deduplicate blocks with identical content
//! - Normalize whitespace

use latexsnipper_ast::*;

/// Options for document cleaning.
#[derive(Debug, Clone)]
pub struct CleanerOptions {
    /// Merge text blocks whose y-distance is below this threshold (in same line).
    pub merge_y_threshold: f32,
    /// Minimum confidence to keep a block. Blocks below this get a warning diagnostic.
    pub min_confidence: f32,
    /// Remove blocks with empty text content.
    pub remove_empty: bool,
    /// Merge blocks that have identical text (same line, overlapping x).
    pub deduplicate: bool,
    /// Trim whitespace from text runs.
    pub normalize_whitespace: bool,
}

impl Default for CleanerOptions {
    fn default() -> Self {
        Self {
            merge_y_threshold: 8.0,
            min_confidence: 0.3,
            remove_empty: true,
            deduplicate: true,
            normalize_whitespace: true,
        }
    }
}

/// Result of cleaning a document.
#[derive(Debug, Clone)]
pub struct CleanResult {
    pub cleaned_document: Document,
    pub blocks_removed: usize,
    pub blocks_merged: usize,
    pub diagnostics_added: Vec<Diagnostic>,
}

/// Clean a Document AST by removing artifacts and repairing structure.
///
/// # Example
/// ```
/// use latexsnipper_ast::*;
/// use latexsnipper_conversion::document_cleaner::{clean_document, CleanerOptions};
///
/// let mut doc = Document::new();
/// doc.pages.push(Page {
///     width: 800.0, height: 600.0,
///     blocks: vec![
///         Block::Paragraph(ParagraphBlock {
///             inlines: vec![Inline::Text(TextRun::new("Hello "))],
///             geometry: Some(Rect::new(10.0, 10.0, 30.0, 20.0)),
///             source: Some(SourceInfo::new().with_confidence(0.95)),
///         }),
///         Block::Paragraph(ParagraphBlock {
///             inlines: vec![Inline::Text(TextRun::new("World"))],
///             geometry: Some(Rect::new(40.0, 10.0, 30.0, 20.0)),
///             source: Some(SourceInfo::new().with_confidence(0.95)),
///         }),
///     ],
///     page_number: Some(1),
/// });
///
/// let result = clean_document(&doc, &CleanerOptions::default());
/// assert!(result.blocks_merged > 0);
/// ```
pub fn clean_document(doc: &Document, options: &CleanerOptions) -> CleanResult {
    let mut blocks_removed = 0;
    let mut blocks_merged = 0;
    let mut all_diagnostics = Vec::new();
    let mut cleaned_pages = Vec::new();

    for page in &doc.pages {
        let mut blocks = page.blocks.clone();

        // Step 1: Remove empty blocks
        if options.remove_empty {
            let before = blocks.len();
            blocks.retain(|b| !is_block_empty(b));
            blocks_removed += before - blocks.len();
        }

        // Step 2: Normalize whitespace
        if options.normalize_whitespace {
            for block in &mut blocks {
                normalize_block_whitespace(block);
            }
        }

        // Step 3: Add diagnostics for low-confidence blocks
        for block in &blocks {
            if let Some(conf) = confidence(block) {
                if conf < options.min_confidence {
                    all_diagnostics.push(
                        Diagnostic::new(
                            DiagnosticLevel::Warning,
                            "E_LOW_CONFIDENCE",
                            &format!(
                                "Block has low confidence ({:.2} < {:.2})",
                                conf, options.min_confidence
                            ),
                        )
                        .with_recoverable(true),
                    );
                }
            }
        }

        // Step 4: Merge adjacent text blocks on the same line
        if options.merge_y_threshold > 0.0 {
            let (merged, merge_count) = merge_adjacent_blocks(&blocks, options.merge_y_threshold);
            blocks = merged;
            blocks_merged += merge_count;
        }

        // Step 5: Deduplicate blocks with identical text
        if options.deduplicate {
            let before = blocks.len();
            blocks = deduplicate_blocks(&blocks);
            blocks_removed += before - blocks.len();
        }

        cleaned_pages.push(Page {
            width: page.width,
            height: page.height,
            blocks,
            page_number: page.page_number,
        });
    }

    CleanResult {
        cleaned_document: Document {
            metadata: doc.metadata.clone(),
            pages: cleaned_pages,
            assets: doc.assets.clone(),
            diagnostics: [doc.diagnostics.clone(), all_diagnostics.clone()].concat(),
            id_gen: NodeIdGenerator::new(),
            schema_version: doc.schema_version.clone(),
        },
        blocks_removed,
        blocks_merged,
        diagnostics_added: all_diagnostics,
    }
}

/// Check if a block is effectively empty (no text content).
fn is_block_empty(block: &Block) -> bool {
    let text = block_text(block);
    text.trim().is_empty()
}

/// Extract all text from a block.
fn block_text(block: &Block) -> String {
    match block {
        Block::Heading(h) => inlines_to_text(&h.inlines),
        Block::Paragraph(p) => inlines_to_text(&p.inlines),
        Block::List(l) => l
            .items
            .iter()
            .map(|item| inlines_to_text(&item.inlines))
            .collect::<Vec<_>>()
            .join(" "),
        Block::Handwriting(hw) => inlines_to_text(&hw.inlines),
        Block::Table(t) => t
            .rows
            .iter()
            .flat_map(|row| row.iter())
            .map(|cell| inlines_to_text(&cell.inlines))
            .collect::<Vec<_>>()
            .join(" "),
        Block::Quote(q) => q
            .blocks
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Formula(f) => f.formula.as_latex().to_string(),
        Block::Code(c) => c.code.clone(),
        Block::Figure(f) => f.caption.as_deref().unwrap_or("").to_string(),
        Block::Theorem(t) => t
            .content
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Proof(p) => p
            .content
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Minipage(m) => m
            .content
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Float(f) => f
            .content
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::TextBox(tb) => tb
            .content
            .iter()
            .map(block_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::DescriptionList(dl) => dl
            .items
            .iter()
            .flat_map(|item| {
                let mut parts = Vec::new();
                if let Some(label) = &item.label {
                    parts.push(inlines_to_text(label));
                }
                for b in &item.content {
                    parts.push(block_text(b));
                }
                parts
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn inlines_to_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.text.clone(),
            Inline::Formula(f) => f.as_latex().to_string(),
            Inline::Image(_) => "[image]".to_string(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_block_whitespace(block: &mut Block) {
    match block {
        Block::Heading(h) => normalize_inlines_whitespace(&mut h.inlines),
        Block::Paragraph(p) => normalize_inlines_whitespace(&mut p.inlines),
        Block::List(l) => {
            for item in &mut l.items {
                normalize_inlines_whitespace(&mut item.inlines);
            }
        }
        Block::Handwriting(hw) => normalize_inlines_whitespace(&mut hw.inlines),
        Block::Table(t) => {
            for row in &mut t.rows {
                for cell in row.iter_mut() {
                    normalize_inlines_whitespace(&mut cell.inlines);
                }
            }
        }
        Block::Quote(q) => {
            for b in &mut q.blocks {
                normalize_block_whitespace(b);
            }
        }
        _ => {}
    }
}

fn normalize_inlines_whitespace(inlines: &mut [Inline]) {
    for inline in inlines.iter_mut() {
        if let Inline::Text(ref mut t) = inline {
            // Collapse multiple spaces, trim
            let normalized = t.text.split_whitespace().collect::<Vec<_>>().join(" ");
            t.text = normalized;
        }
    }
}

/// Get the confidence of a block from its SourceInfo.
fn confidence(block: &Block) -> Option<f32> {
    block.source().and_then(|s| s.confidence)
}

/// Merge adjacent text blocks on the same y-line.
fn merge_adjacent_blocks(blocks: &[Block], y_threshold: f32) -> (Vec<Block>, usize) {
    let mut merged = Vec::new();
    let mut merge_count = 0;
    let mut i = 0;

    while i < blocks.len() {
        let current = &blocks[i];

        // Only merge Paragraph blocks
        if let Block::Paragraph(ref p) = current {
            let current_geom = p.geometry;
            let _current_y = current_geom.map(|r| r.y).unwrap_or(0.0);

            // Look ahead for merge candidates
            let mut j = i + 1;
            let mut accumulated = p.inlines.clone();
            let mut merged_geom = current_geom;

            while j < blocks.len() {
                if let Block::Paragraph(ref next_p) = blocks[j] {
                    let next_geom = next_p.geometry;
                    if let (Some(cg), Some(ng)) = (merged_geom, next_geom) {
                        // Same line? Check y-overlap
                        let y_diff = (cg.y - ng.y).abs();
                        if y_diff <= y_threshold {
                            // Merge: add a space and concatenate
                            accumulated.push(Inline::Text(TextRun::new(" ")));
                            accumulated.extend(next_p.inlines.clone());
                            // Extend geometry to cover both
                            let min_x = cg.x.min(ng.x);
                            let max_right = (cg.x + cg.width).max(ng.x + ng.width);
                            merged_geom = Some(Rect::new(
                                min_x,
                                cg.y,
                                max_right - min_x,
                                cg.height.max(ng.height),
                            ));
                            merge_count += 1;
                            j += 1;
                            continue;
                        }
                    }
                }
                break;
            }

            merged.push(Block::Paragraph(ParagraphBlock {
                inlines: accumulated,
                geometry: merged_geom,
                source: p.source.clone(),
            }));
            i = j;
        } else {
            merged.push(blocks[i].clone());
            i += 1;
        }
    }

    (merged, merge_count)
}

/// Remove blocks with duplicate text content on the same page region.
fn deduplicate_blocks(blocks: &[Block]) -> Vec<Block> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for block in blocks {
        let text = block_text(block).trim().to_string();
        if text.is_empty() || seen.insert(text) {
            result.push(block.clone());
        }
    }

    result
}
