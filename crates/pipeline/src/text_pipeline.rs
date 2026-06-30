use latexsnipper_foundation::Result;
use latexsnipper_ast::{Document, Page, Block, ParagraphBlock, Inline, TextRun};

use crate::context::PipelineContext;

/// Format text blocks into document.
pub fn format_text_output(ctx: &mut PipelineContext) -> Result<()> {
    let doc = &ctx.document;

    // Collect all text content
    let mut texts = Vec::new();
    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Paragraph(p) => {
                    let text: String = p.inlines.iter().filter_map(|i| {
                        if let Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
                    }).collect();
                    if !text.is_empty() {
                        texts.push(text);
                    }
                }
                _ => {}
            }
        }
    }

    let formatted = texts.join("\n");
    ctx.set("formatted_text", serde_json::Value::String(formatted));
    Ok(())
}

/// Create a document from recognized text lines.
pub fn build_document_from_text(lines: Vec<(String, f32)>) -> Document {
    let blocks: Vec<Block> = lines.into_iter().map(|(text, confidence)| {
        Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun {
                text,
                bold: None,
                italic: None,
            })],
            geometry: None,
            source: None,
        })
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
