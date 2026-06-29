use latexsnipper_foundation::Result;
use latexsnipper_ast::{Document, Page, Block, FormulaBlock, Formula, ParagraphBlock, Inline, TextRun};

use crate::context::PipelineContext;

/// Format formula blocks into LaTeX output.
pub fn format_formula_output(ctx: &mut PipelineContext) -> Result<()> {
    let doc = &ctx.document;

    // Collect all formula blocks
    let mut formulas = Vec::new();
    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Formula(f) => {
                    formulas.push(f.formula.as_latex().to_string());
                }
                Block::Paragraph(p) => {
                    for inline in &p.inlines {
                        if let Inline::Formula(f) = inline {
                            formulas.push(f.as_latex().to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Store formatted text in metadata
    let formatted = if formulas.is_empty() {
        String::new()
    } else {
        formulas.join("\n\n")
    };

    ctx.set("formatted_text", serde_json::Value::String(formatted));
    Ok(())
}

/// Create a simple document from detected formulas and text.
pub fn build_document(
    formulas: Vec<(String, f32)>,
    texts: Vec<(String, f32)>,
) -> Document {
    let mut blocks = Vec::new();

    for (text, confidence) in texts {
        blocks.push(Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun {
                text,
                bold: None,
                italic: None,
            })],
            geometry: None,
        }));
    }

    for (latex, confidence) in formulas {
        blocks.push(Block::Formula(FormulaBlock {
            formula: Formula {
                source: latexsnipper_ast::FormulaSource::Latex(latex),
                display_mode: true,
                confidence,
            },
            geometry: None,
        }));
    }

    Document {
        metadata: latexsnipper_ast::Metadata::default(),
        pages: vec![Page {
            width: 0.0,
            height: 0.0,
            blocks,
            page_number: None,
        }],
    }
}
