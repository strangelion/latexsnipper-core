use crate::{
    Block, CodeBlock, Document, Formula, FormulaBlock, HeadingBlock, Inline, ListBlock, ListItem,
    Metadata, Page, ParagraphBlock, QuoteBlock, TextRun,
};

/// Builder for constructing Documents fluently.
///
/// # Example
/// ```
/// use latexsnipper_ast::DocumentBuilder;
///
/// let doc = DocumentBuilder::new()
///     .page(800.0, 600.0, |page| {
///         page.heading(1, "Hello World");
///         page.paragraph(|p| {
///             p.text("This is a ");
///             p.formula("\\frac{a}{b}");
///             p.text(" equation.");
///         });
///         page.display_formula("\\sum_{i=1}^{n} x_i");
///         page.code("rust", "fn main() {}");
///     })
///     .build();
///
/// assert_eq!(doc.block_count(), 4);
/// ```
pub struct DocumentBuilder {
    doc: Document,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            doc: Document::new(),
        }
    }

    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.doc.metadata = metadata;
        self
    }

    pub fn page<F>(mut self, width: f32, height: f32, f: F) -> Self
    where
        F: FnOnce(&mut PageBuilder),
    {
        let mut page_builder = PageBuilder::new(width, height);
        f(&mut page_builder);
        self.doc.pages.push(page_builder.build());
        self
    }

    pub fn build(self) -> Document {
        self.doc
    }
}

impl Default for DocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a Page.
pub struct PageBuilder {
    width: f32,
    height: f32,
    blocks: Vec<Block>,
}

impl PageBuilder {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            blocks: Vec::new(),
        }
    }

    pub fn heading(&mut self, level: u8, text: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Heading(HeadingBlock {
            level,
            inlines: vec![Inline::Text(TextRun::new(text))],
            id: None,
            geometry: None,
            source: None,
        }));
        self
    }

    pub fn paragraph<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut ParagraphBuilder),
    {
        let mut builder = ParagraphBuilder::new();
        f(&mut builder);
        self.blocks.push(Block::Paragraph(builder.build()));
        self
    }

    pub fn text_paragraph(&mut self, text: impl Into<String>) -> &mut Self {
        self.paragraph(|p| {
            p.text(text);
        })
    }

    pub fn formula(&mut self, latex: impl Into<String>) -> &mut Self {
        self.paragraph(|p| {
            p.formula(latex);
        })
    }

    pub fn display_formula(&mut self, latex: impl Into<String>) -> &mut Self {
        let formula = Formula::latex(latex);
        self.blocks.push(Block::Formula(FormulaBlock {
            formula,
            geometry: None,
            source: None,
        }));
        self
    }

    pub fn code(&mut self, language: impl Into<String>, code: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Code(CodeBlock {
            language: Some(language.into()),
            code: code.into(),
            geometry: None,
            source: None,
        }));
        self
    }

    pub fn unordered_list<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut ListBuilder),
    {
        let mut builder = ListBuilder::new(false);
        f(&mut builder);
        self.blocks.push(Block::List(builder.build()));
        self
    }

    pub fn ordered_list<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut ListBuilder),
    {
        let mut builder = ListBuilder::new(true);
        f(&mut builder);
        self.blocks.push(Block::List(builder.build()));
        self
    }

    pub fn quote<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut QuoteBuilder),
    {
        let mut builder = QuoteBuilder::new();
        f(&mut builder);
        self.blocks.push(Block::Quote(builder.build()));
        self
    }

    pub fn push_block(&mut self, block: Block) -> &mut Self {
        self.blocks.push(block);
        self
    }

    fn build(self) -> Page {
        Page {
            width: self.width,
            height: self.height,
            blocks: self.blocks,
            page_number: None,
        }
    }
}

/// Builder for constructing a Paragraph.
pub struct ParagraphBuilder {
    inlines: Vec<Inline>,
}

impl ParagraphBuilder {
    pub fn new() -> Self {
        Self {
            inlines: Vec::new(),
        }
    }

    pub fn text(&mut self, text: impl Into<String>) -> &mut Self {
        self.inlines.push(Inline::Text(TextRun::new(text)));
        self
    }

    pub fn bold_text(&mut self, text: impl Into<String>) -> &mut Self {
        self.inlines
            .push(Inline::Text(TextRun::new(text).with_bold(true)));
        self
    }

    pub fn italic_text(&mut self, text: impl Into<String>) -> &mut Self {
        self.inlines
            .push(Inline::Text(TextRun::new(text).with_italic(true)));
        self
    }

    pub fn formula(&mut self, latex: impl Into<String>) -> &mut Self {
        self.inlines.push(Inline::Formula(Formula::latex(latex)));
        self
    }

    fn build(self) -> ParagraphBlock {
        ParagraphBlock {
            inlines: self.inlines,
            geometry: None,
            source: None,
        }
    }
}

impl Default for ParagraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a List.
pub struct ListBuilder {
    ordered: bool,
    items: Vec<ListItem>,
}

impl ListBuilder {
    pub fn new(ordered: bool) -> Self {
        Self {
            ordered,
            items: Vec::new(),
        }
    }

    pub fn item<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut ParagraphBuilder),
    {
        let mut builder = ParagraphBuilder::new();
        f(&mut builder);
        self.items.push(ListItem {
            inlines: builder.build().inlines,
            checked: None,
            source: None,
        });
        self
    }

    pub fn text_item(&mut self, text: impl Into<String>) -> &mut Self {
        self.item(|p| {
            p.text(text);
        })
    }

    fn build(self) -> ListBlock {
        ListBlock {
            ordered: self.ordered,
            items: self.items,
            geometry: None,
            source: None,
        }
    }
}

/// Builder for constructing a Quote.
pub struct QuoteBuilder {
    blocks: Vec<Block>,
    attribution: Option<String>,
}

impl QuoteBuilder {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            attribution: None,
        }
    }

    pub fn paragraph<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut ParagraphBuilder),
    {
        let mut builder = ParagraphBuilder::new();
        f(&mut builder);
        self.blocks.push(Block::Paragraph(builder.build()));
        self
    }

    pub fn text_paragraph(&mut self, text: impl Into<String>) -> &mut Self {
        self.paragraph(|p| {
            p.text(text);
        })
    }

    pub fn with_attribution(&mut self, attr: impl Into<String>) -> &mut Self {
        self.attribution = Some(attr.into());
        self
    }

    fn build(self) -> QuoteBlock {
        QuoteBlock {
            blocks: self.blocks,
            attribution: self.attribution,
            geometry: None,
            source: None,
        }
    }
}

impl Default for QuoteBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_basic() {
        let doc = DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.heading(1, "Title");
                page.text_paragraph("Hello world");
                page.formula("E = mc^2");
            })
            .build();

        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.block_count(), 3);
    }

    #[test]
    fn builder_complex() {
        let doc = DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.heading(1, "Math Document");
                page.paragraph(|p| {
                    p.text("The equation ");
                    p.formula("\\frac{a}{b}");
                    p.text(" is important.");
                });
                page.display_formula("\\sum_{i=1}^{n} x_i");
                page.unordered_list(|l| {
                    l.text_item("First item");
                    l.text_item("Second item");
                });
                page.code("rust", "fn main() {}");
            })
            .build();

        assert_eq!(doc.block_count(), 5);
    }

    #[test]
    fn builder_quote() {
        let doc = DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.quote(|q| {
                    q.text_paragraph("To be or not to be");
                    q.with_attribution("Shakespeare");
                });
            })
            .build();

        assert_eq!(doc.block_count(), 1);
    }
}
