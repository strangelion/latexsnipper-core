pub mod converter;
pub mod document_converter;
pub mod html;
pub mod latex;
pub mod latex_ast;
pub mod latex_parser;
pub mod latex_to_typst;
pub mod latex_utils;
pub mod markdown;
pub mod markdown_parser;
pub mod mathml;
pub mod mathml_parser;
pub mod omml;
pub mod omml_parser;
pub mod typst;
pub mod typst_parser;

pub use converter::Converter;
pub use document_converter::{DocumentConverter, OutputFormat};
pub use html::HtmlConverter;
pub use latex::{LatexConverter, LatexDisplayConverter, LatexEquationConverter};
pub use markdown::{MarkdownBlockConverter, MarkdownInlineConverter};
pub use markdown_parser::parse_markdown_to_document;
pub use mathml::{MathmlAttrConverter, MathmlConverter, MathmlMConverter, MathmlMmlConverter};
pub use mathml_parser::parse_mathml_to_latex;
pub use omml::OmmlConverter;
pub use omml_parser::parse_omml_to_latex;
pub use typst::TypstConverter;
pub use typst_parser::parse_typst_to_latex;

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::{
        Block, Document, Formula, FormulaBlock, Inline, Page, ParagraphBlock, TextRun,
    };

    fn test_doc() -> Document {
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

    #[test]
    fn latex_converter() {
        let doc = test_doc();
        let converter = LatexConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("Given the equation"));
        assert!(result.contains("E=mc^2"));
        assert!(result.contains("\\frac{a+b}{c}"));
        assert_eq!(converter.name(), "latex");
        assert_eq!(converter.extension(), "tex");
    }

    #[test]
    fn latex_display_converter() {
        let doc = test_doc();
        let converter = LatexDisplayConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("\\["));
        assert!(result.contains("\\]"));
        assert!(result.contains("E=mc^2"));
        assert_eq!(converter.name(), "latex_display");
    }

    #[test]
    fn latex_equation_converter() {
        let doc = test_doc();
        let converter = LatexEquationConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("\\begin{equation}"));
        assert!(result.contains("\\end{equation}"));
        assert!(result.contains("E=mc^2"));
        assert_eq!(converter.name(), "latex_equation");
    }

    #[test]
    fn typst_converter() {
        let doc = test_doc();
        let converter = TypstConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("Given the equation"));
        assert!(result.contains("E") && result.contains("m") && result.contains("c"));
        assert!(
            result.contains("frac") || result.contains("(a+b)/(c)") || result.contains("(a, b)")
        );
        assert_eq!(converter.name(), "typst");
        assert_eq!(converter.extension(), "typ");
    }

    #[test]
    fn markdown_inline_converter() {
        let doc = test_doc();
        let converter = MarkdownInlineConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("$E=mc^2$") || result.contains("E=mc^2"));
        assert!(result.contains("frac") || result.contains("\\frac"));
        assert_eq!(converter.name(), "markdown_inline");
        assert_eq!(converter.extension(), "md");
    }

    #[test]
    fn markdown_block_converter() {
        let doc = test_doc();
        let converter = MarkdownBlockConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("$$"));
        assert!(result.contains("E=mc^2"));
        assert_eq!(converter.name(), "markdown_block");
        assert_eq!(converter.extension(), "md");
    }

    #[test]
    fn mathml_converter() {
        let doc = test_doc();
        let converter = MathmlConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("<math"));
        assert!(result.contains("E=mc^2"));
        assert!(result.contains("<mfrac>"));
        assert_eq!(converter.name(), "mathml");
    }

    #[test]
    fn mathml_mml_converter() {
        let doc = test_doc();
        let converter = MathmlMmlConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("mml:math"));
        assert_eq!(converter.name(), "mathml_mml");
    }

    #[test]
    fn mathml_m_converter() {
        let doc = test_doc();
        let converter = MathmlMConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("<m:math"));
        assert_eq!(converter.name(), "mathml_m");
    }

    #[test]
    fn mathml_attr_converter() {
        let doc = test_doc();
        let converter = MathmlAttrConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("math"));
        assert_eq!(converter.name(), "mathml_attr");
    }

    #[test]
    fn omml_converter() {
        let doc = test_doc();
        let converter = OmmlConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("E=mc^2"));
        assert!(result.contains("<m:f>"));
        assert_eq!(converter.name(), "omml");
    }

    #[test]
    fn html_converter() {
        let doc = test_doc();
        let converter = HtmlConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("MathJax"));
        assert!(result.contains("E=mc^2"));
        assert!(result.contains("$"));
        assert_eq!(converter.name(), "html");
        assert_eq!(converter.extension(), "html");
    }

    #[test]
    fn latex_fraction_omml() {
        let doc = Document {
            metadata: latexsnipper_ast::Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![Block::Formula(FormulaBlock {
                    formula: Formula::latex("\\frac{a}{b}"),
                    geometry: None,
                    source: None,
                })],
                page_number: None,
            }],
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        };
        let converter = OmmlConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("<m:f>"));
        assert!(result.contains("<m:num>"));
        assert!(result.contains("<m:den>"));
    }

    #[test]
    fn latex_fraction_mathml() {
        let doc = Document {
            metadata: latexsnipper_ast::Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![Block::Formula(FormulaBlock {
                    formula: Formula::latex("\\frac{a}{b}"),
                    geometry: None,
                    source: None,
                })],
                page_number: None,
            }],
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        };
        let converter = MathmlConverter;
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("<mfrac>"));
    }
}
