use crate::{Block, Document, Inline, Page};

/// Visitor pattern for traversing and transforming Documents.
/// Used by Renderers and Parsers.
pub trait DocumentVisitor<T> {
    fn visit_document(&mut self, doc: &Document) -> T;
    fn visit_page(&mut self, page: &Page) -> T;
    fn visit_block(&mut self, block: &Block) -> T;
    fn visit_inline(&mut self, inline: &Inline) -> T;
}

/// A simple visitor that collects text content.
pub struct TextCollector {
    pub text: String,
}

impl TextCollector {
    pub fn new() -> Self {
        Self { text: String::new() }
    }
}

impl Default for TextCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentVisitor<()> for TextCollector {
    fn visit_document(&mut self, doc: &Document) {
        for page in &doc.pages {
            self.visit_page(page);
        }
    }

    fn visit_page(&mut self, page: &Page) {
        for block in &page.blocks {
            self.visit_block(block);
        }
    }

    fn visit_block(&mut self, block: &Block) {
        match block {
            Block::Paragraph(p) => {
                for inline in &p.inlines {
                    self.visit_inline(inline);
                }
            }
            Block::Formula(f) => {
                self.text.push_str(f.formula.as_latex());
            }
            _ => {}
        }
    }

    fn visit_inline(&mut self, inline: &Inline) {
        match inline {
            Inline::Text(t) => {
                self.text.push_str(&t.text);
            }
            Inline::Formula(f) => {
                self.text.push_str(f.as_latex());
            }
            _ => {}
        }
    }
}
