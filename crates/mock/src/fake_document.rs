use latexsnipper_ast::{Document, Page, Block, FormulaBlock, Formula, ParagraphBlock, Inline, TextRun};

/// Create a fake document with mixed formula and text content.
pub fn fake_document() -> Document {
    Document {
        metadata: latexsnipper_ast::Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![Inline::Text(TextRun::new("Given the equation "))],
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: {
                        let mut f = Formula::latex("E=mc^2");
                        f.display_mode = false;
                        f.confidence = 0.95;
                        f
                    },
                    geometry: None,
                    source: None,
                }),
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![Inline::Text(TextRun::new(", we can derive the following:"))],
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: {
                        let mut f = Formula::latex("\\frac{a+b}{c}");
                        f.confidence = 0.92;
                        f
                    },
                    geometry: None,
                    source: None,
                }),
            ],
            page_number: Some(1),
        }],
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_document_has_blocks() {
        let doc = fake_document();
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.pages[0].blocks.len(), 4);
    }

    #[test]
    fn fake_document_has_formula() {
        let doc = fake_document();
        let formulas: Vec<_> = doc.pages[0].blocks.iter().filter_map(|b| {
            if let Block::Formula(f) = b { Some(f) } else { None }
        }).collect();
        assert_eq!(formulas.len(), 2);
        assert_eq!(formulas[0].formula.as_latex(), "E=mc^2");
        assert_eq!(formulas[1].formula.as_latex(), "\\frac{a+b}{c}");
    }
}
