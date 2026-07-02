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
        assert!(
            result.contains("<m:t>E=mc</m:t>"),
            "should contain E=mc: {}",
            result
        );
        assert!(
            result.contains("<m:sSup>"),
            "should have superscript: {}",
            result
        );
        assert!(result.contains("<m:f>"), "should have fraction: {}", result);
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

    /// Test that LaTeX → all output formats produces proper math structures (not plain text).
    #[test]
    fn conversion_matrix_latex_to_all() {
        let formulas = vec![
            ("\\frac{a+b}{c}", "fraction"),
            ("x^{2}+y_{i}", "superscript+subscript"),
            ("\\sqrt{x}", "square root"),
            ("\\alpha+\\beta", "greek letters"),
            ("\\lim_{x \\to 0}", "function with limit"),
            ("\\int_{0}^{1} f(x) dx", "integral"),
            ("\\sum_{i=1}^{n} a_i", "sum"),
            ("\\hat{x}+\\bar{y}", "accents"),
            ("\\operatorname{Spec}(A)", "operatorname"),
            ("\\left(\\frac{a}{b}\\right)", "delimited fraction"),
        ];

        for (latex, desc) in &formulas {
            let result_latex =
                DocumentConverter::convert_latex_string(latex, OutputFormat::Latex).unwrap();
            assert!(!result_latex.is_empty(), "LaTeX→LaTeX empty for {}", desc);

            let result_typst =
                DocumentConverter::convert_latex_string(latex, OutputFormat::Typst).unwrap();
            assert!(!result_typst.is_empty(), "LaTeX→Typst empty for {}", desc);

            let result_mathml =
                DocumentConverter::convert_latex_string(latex, OutputFormat::MathML).unwrap();
            assert!(
                result_mathml.contains("<math"),
                "LaTeX→MathML missing <math> for {}: {}",
                desc,
                result_mathml
            );

            let result_omml =
                DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap();
            // OMML should have math elements, not just plain text
            let has_math_structure = result_omml.contains("<m:f>")
                || result_omml.contains("<m:sSup>")
                || result_omml.contains("<m:sSub>")
                || result_omml.contains("<m:rad>")
                || result_omml.contains("<m:acc>")
                || result_omml.contains("<m:func>")
                || result_omml.contains("<m:nary>")
                || result_omml.contains("<m:d>")
                || result_omml.contains("<m:bar>")
                || result_omml.contains("<m:mRow>")
                || result_omml.contains("<m:oMathPara");
            assert!(
                has_math_structure,
                "LaTeX→OMML missing math structure for {}: {}",
                desc, result_omml
            );

            let result_md =
                DocumentConverter::convert_latex_string(latex, OutputFormat::MarkdownBlock)
                    .unwrap();
            assert!(
                result_md.contains("$") || result_md.contains("\\("),
                "LaTeX→Markdown missing math delimiters for {}: {}",
                desc,
                result_md
            );

            let result_html =
                DocumentConverter::convert_latex_string(latex, OutputFormat::Html).unwrap();
            assert!(
                result_html.contains("MathJax") || result_html.contains("math"),
                "LaTeX→HTML missing math for {}: {}",
                desc,
                result_html
            );
        }
    }

    /// Test roundtrip: OMML → LaTeX → OMML preserves math structure.
    #[test]
    fn roundtrip_omml_latex_omml() {
        let omml = r#"<m:oMathPara><m:oMath><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath></m:oMathPara>"#;
        let latex = DocumentConverter::convert_omml_string(omml, OutputFormat::Latex).unwrap();
        assert!(
            latex.contains("frac"),
            "OMML→LaTeX should produce frac: {}",
            latex
        );

        let omml2 = DocumentConverter::convert_latex_string(&latex, OutputFormat::OMML).unwrap();
        assert!(
            omml2.contains("<m:f>"),
            "Roundtrip OMML→LaTeX→OMML lost fraction: {}",
            omml2
        );
        assert!(
            omml2.contains("<m:num>"),
            "Roundtrip missing numerator: {}",
            omml2
        );
        assert!(
            omml2.contains("<m:den>"),
            "Roundtrip missing denominator: {}",
            omml2
        );
    }

    /// Test roundtrip: MathML → LaTeX → MathML preserves math structure.
    #[test]
    fn roundtrip_mathml_latex_mathml() {
        let mathml = r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><msup><mi>x</mi><mn>2</mn></msup></math>"#;
        let latex = DocumentConverter::convert_mathml_string(mathml, OutputFormat::Latex).unwrap();
        assert!(
            latex.contains("^") || latex.contains("x^"),
            "MathML→LaTeX should produce superscript: {}",
            latex
        );

        let mathml2 =
            DocumentConverter::convert_latex_string(&latex, OutputFormat::MathML).unwrap();
        assert!(
            mathml2.contains("<msup>"),
            "Roundtrip MathML→LaTeX→MathML lost superscript: {}",
            mathml2
        );
    }

    /// Test roundtrip: Typst → LaTeX → Typst preserves math structure.
    #[test]
    fn roundtrip_typst_latex_typst() {
        let typst = r#"frac(a, b)"#;
        let latex = DocumentConverter::convert_typst_string(typst, OutputFormat::Latex).unwrap();
        assert!(
            latex.contains("frac") || latex.contains("over"),
            "Typst→LaTeX should produce fraction: {}",
            latex
        );

        let typst2 = DocumentConverter::convert_latex_string(&latex, OutputFormat::Typst).unwrap();
        assert!(
            typst2.contains("frac"),
            "Roundtrip Typst→LaTeX→Typst lost fraction: {}",
            typst2
        );
    }
}
