use crate::{Block, Document, Inline, Page};

/// Visitor pattern for traversing and transforming Documents.
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
        Self {
            text: String::new(),
        }
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
            Block::Heading(h) => {
                for inline in &h.inlines {
                    self.visit_inline(inline);
                }
                self.text.push('\n');
            }
            Block::Paragraph(p) => {
                for inline in &p.inlines {
                    self.visit_inline(inline);
                }
                self.text.push('\n');
            }
            Block::Formula(f) => {
                self.text.push_str(f.formula.as_latex());
                self.text.push('\n');
            }
            Block::List(l) => {
                for item in &l.items {
                    for inline in &item.inlines {
                        self.visit_inline(inline);
                    }
                    self.text.push('\n');
                }
            }
            Block::Quote(q) => {
                for b in &q.blocks {
                    self.visit_block(b);
                }
            }
            Block::Code(c) => {
                self.text.push_str(&c.code);
                self.text.push('\n');
            }
            Block::HorizontalRule(_) => {
                self.text.push_str("---\n");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentBuilder;

    #[test]
    fn text_collector_basic() {
        let doc = DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.heading(1, "Title");
                page.text_paragraph("Hello world");
                page.formula("E = mc^2");
            })
            .build();

        let mut collector = TextCollector::new();
        collector.visit_document(&doc);

        assert!(collector.text.contains("Title"));
        assert!(collector.text.contains("Hello world"));
        assert!(collector.text.contains("E = mc^2"));
    }
}
