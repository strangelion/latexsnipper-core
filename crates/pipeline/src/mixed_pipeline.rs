use latexsnipper_foundation::Result;
use latexsnipper_ast::{Document, Page, Block, FormulaBlock, Formula, ParagraphBlock, Inline, TextRun};

use crate::context::PipelineContext;

/// Format mixed (formula + text) output.
pub fn format_mixed_output(ctx: &mut PipelineContext) -> Result<()> {
    let doc = &ctx.document;

    let mut output = Vec::new();

    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Formula(f) => {
                    let latex = f.formula.as_latex();
                    if f.formula.display_mode {
                        output.push(format!("$$\n{}\n$$", latex));
                    } else {
                        output.push(format!("${}$", latex));
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
                        output.push(text);
                    }
                }
                _ => {}
            }
        }
    }

    let formatted = output.join("\n\n");
    ctx.set("formatted_text", serde_json::Value::String(formatted));
    Ok(())
}

/// Create a document from mixed formula and text results.
pub fn build_document_from_mixed(
    regions: Vec<(RegionType, String, f32)>,
) -> Document {
    let blocks: Vec<Block> = regions.into_iter().map(|(region_type, content, confidence)| {
        match region_type {
            RegionType::Formula => Block::Formula(FormulaBlock {
                formula: Formula {
                    source: latexsnipper_ast::FormulaSource::Latex(content),
                    display_mode: true,
                    confidence,
                },
                geometry: None,
                source: None,
            }),
            RegionType::Text => Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun {
                    text: content,
                    bold: None,
                    italic: None,
                })],
                geometry: None,
                source: None,
            }),
        }
    }).collect();

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

/// Region type for mixed recognition.
#[derive(Debug, Clone, Copy)]
pub enum RegionType {
    Formula,
    Text,
}
