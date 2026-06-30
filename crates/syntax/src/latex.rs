use latexsnipper_ast::{Document, Page, Block, FormulaBlock, Formula, FormulaSource, ParagraphBlock, Inline, TextRun};
use latexsnipper_foundation::{SnipperError, Result};

use crate::parser::Parser;
use crate::renderer::Renderer;

/// LaTeX parser — converts LaTeX string to Document AST.
pub struct LatexParser;

impl Parser for LatexParser {
    fn parse(&self, input: &str) -> Result<Document> {
        let blocks = parse_latex_content(input);
        Ok(Document {
            metadata: latexsnipper_ast::Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks,
                page_number: None,
            }],
        })
    }

    fn name(&self) -> &str { "latex" }
}

/// LaTeX renderer — converts Document AST to LaTeX string.
pub struct LatexRenderer;

impl Renderer for LatexRenderer {
    fn render(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();
        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => {
                        let latex = f.formula.as_latex();
                        if f.formula.display_mode {
                            parts.push(format!("$$\n{}\n$$", latex));
                        } else {
                            parts.push(format!("${}$", latex));
                        }
                    }
                    Block::Paragraph(p) => {
                        let text: String = p.inlines.iter().map(|i| {
                            match i {
                                Inline::Text(t) => t.text.clone(),
                                Inline::Formula(f) => {
                                    if f.display_mode {
                                        format!("$$\n{}\n$$", f.as_latex())
                                    } else {
                                        format!("${}$", f.as_latex())
                                    }
                                }
                                _ => String::new(),
                            }
                        }).collect();
                        if !text.is_empty() {
                            parts.push(text);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(parts.join("\n\n"))
    }

    fn name(&self) -> &str { "latex" }
}

/// Parse LaTeX content into blocks.
fn parse_latex_content(input: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        // Try to find display math $$...$$
        if let Some(start) = remaining.find("$$") {
            let after_start = &remaining[start + 2..];
            if let Some(end) = after_start.find("$$") {
                let formula = after_start[..end].trim().to_string();
                // Add any text before the formula
                let before = remaining[..start].trim();
                if !before.is_empty() {
                    blocks.push(Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun {
                            text: before.to_string(),
                            bold: None,
                            italic: None,
                        })],
                        geometry: None,
                        source: None,
                    }));
                }
                blocks.push(Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: FormulaSource::Latex(formula),
                        display_mode: true,
                        confidence: 1.0,
                    },
                    geometry: None,
                    source: None,
                }));
                remaining = &after_start[end + 2..];
                continue;
            }
        }

        // Try to find inline math $...$
        if let Some(start) = remaining.find('$') {
            // Make sure it's not $$
            if start + 1 < remaining.len() && remaining.as_bytes()[start + 1] == b'$' {
                // Skip $$, will be handled above
                remaining = &remaining[start + 2..];
                continue;
            }
            let after_start = &remaining[start + 1..];
            if let Some(end) = after_start.find('$') {
                let formula = after_start[..end].trim().to_string();
                let before = remaining[..start].trim();
                if !before.is_empty() {
                    blocks.push(Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun {
                            text: before.to_string(),
                            bold: None,
                            italic: None,
                        })],
                        geometry: None,
                        source: None,
                    }));
                }
                blocks.push(Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: FormulaSource::Latex(formula),
                        display_mode: false,
                        confidence: 1.0,
                    },
                    geometry: None,
                    source: None,
                }));
                remaining = &after_start[end + 1..];
                continue;
            }
        }

        // No more math, treat rest as text
        let text = remaining.trim().to_string();
        if !text.is_empty() {
            blocks.push(Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun {
                    text,
                    bold: None,
                    italic: None,
                })],
                geometry: None,
                source: None,
            }));
        }
        break;
    }

    blocks
}
