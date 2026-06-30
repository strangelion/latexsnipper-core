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
                    inlines: vec![Inline::Text(TextRun {
                        text: "Given the equation ".into(),
                        bold: None,
                        italic: None,
                    })],
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: latexsnipper_ast::FormulaSource::Latex("E=mc^2".into()),
                        display_mode: false,
                        confidence: 0.95,
                    },
                    geometry: None,
                    source: None,
                }),
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![Inline::Text(TextRun {
                        text: ", we can derive the following:".into(),
                        bold: None,
                        italic: None,
                    })],
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: latexsnipper_ast::FormulaSource::Latex("\\frac{a+b}{c}".into()),
                        display_mode: true,
                        confidence: 0.92,
                    },
                    geometry: None,
                    source: None,
                }),
            ],
            page_number: Some(1),
        }],
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
