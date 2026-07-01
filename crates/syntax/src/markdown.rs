use latexsnipper_ast::{Block, Document, Inline};
use latexsnipper_foundation::Result;

use crate::renderer::Renderer;

/// Markdown renderer — converts Document AST to Markdown.
pub struct MarkdownRenderer;

impl Renderer for MarkdownRenderer {
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
                        let text: String = p
                            .inlines
                            .iter()
                            .map(|i| match i {
                                Inline::Text(t) => t.text.clone(),
                                Inline::Formula(f) => {
                                    if f.display_mode {
                                        format!("$$\n{}\n$$", f.as_latex())
                                    } else {
                                        format!("${}$", f.as_latex())
                                    }
                                }
                                _ => String::new(),
                            })
                            .collect();
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

    fn name(&self) -> &str {
        "markdown"
    }
}
