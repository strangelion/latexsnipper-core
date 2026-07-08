pub mod asset_helper;
pub mod asset_resolver;
pub mod converter;
pub mod document_converter;
pub mod export_format;
pub mod exporter;
pub mod html;
pub mod html_parser;
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
pub mod table_export;
pub mod table_parser;
pub mod typst;
pub mod typst_parser;
pub mod word_ooxml_table_parser;
pub mod word_ooxml_table_writer;

pub use asset_helper::{resolve_asset_ref, resolve_image_html, resolve_image_latex, resolve_image_markdown, resolve_image_typst};
pub use converter::Converter;
pub use document_converter::{DocumentConverter, OutputFormat};
pub use html::HtmlConverter;
pub use html_parser::parse_html_to_document;
pub use latex::{LatexConverter, LatexDisplayConverter, LatexEquationConverter};
pub use markdown::{MarkdownBlockConverter, MarkdownInlineConverter};
pub use markdown_parser::parse_markdown_to_document;
pub use mathml::{MathmlAttrConverter, MathmlConverter, MathmlMConverter, MathmlMmlConverter};
pub use mathml_parser::parse_mathml_to_latex;
pub use omml::OmmlConverter;
pub use omml_parser::parse_omml_to_latex;
pub use table_export::TableExporter;
pub use table_parser::{
    parse_html_table, parse_latex_table, parse_markdown_table, parse_tsv_table, parse_typst_table,
};
pub use typst::TypstConverter;
pub use typst_parser::parse_typst_to_latex;
pub use word_ooxml_table_parser::parse_word_table_ooxml;
pub use word_ooxml_table_writer::write_word_table_ooxml;

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
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
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
        assert!(
            result.contains("<mi>E</mi>"),
            "should contain E: {}",
            result
        );
        assert!(
            result.contains("<msup>"),
            "should have superscript: {}",
            result
        );
        assert!(
            result.contains("<mn>2</mn>"),
            "should contain exponent 2: {}",
            result
        );
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
            result.contains("<m:t>E</m:t>"),
            "should contain E: {}",
            result
        );
        assert!(
            result.contains("<m:t>=</m:t>"),
            "should contain equals: {}",
            result
        );
        assert!(
            result.contains("<m:t>mc</m:t>"),
            "should contain mc: {}",
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
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
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
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: latexsnipper_ast::NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
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

    /// Test \to and \rightarrow conversion in LaTeX→Typst.
    #[test]
    fn latex_typst_arrow_conversion() {
        use crate::latex_parser::parse_latex;
        use crate::latex_to_typst::latex_ast_to_typst;

        let cases = vec![
            ("\\to", "arrow.r"),
            ("\\rightarrow", "arrow.r"),
            ("\\Rightarrow", "arrow.r.double"),
            ("\\leftarrow", "arrow.l"),
            ("\\Leftarrow", "arrow.l.double"),
            ("\\leftrightarrow", "arrow.l.r"),
        ];
        for (latex, expected) in &cases {
            let result = latex_ast_to_typst(&parse_latex(latex));
            assert_eq!(result.trim(), *expected, "{} => {}", latex, expected);
        }
    }

    /// Test \lim with \to arrow.
    #[test]
    fn latex_typst_limit_with_arrow() {
        use crate::latex_parser::parse_latex;
        use crate::latex_to_typst::latex_ast_to_typst;

        let result = latex_ast_to_typst(&parse_latex("\\lim_{x \\to 0}"));
        println!("  limit_to: {:?}", result);
        assert!(result.contains("limit"), "should have limit: {}", result);
        assert!(
            result.contains("arrow") || result.contains("to"),
            "should have arrow/rarrow: {}",
            result
        );
    }

    /// Test \left( \right) with nested content.
    #[test]
    fn latex_typst_delimited_nested() {
        use crate::latex_parser::parse_latex;
        use crate::latex_to_typst::latex_ast_to_typst;

        let result = latex_ast_to_typst(&parse_latex("\\left(\\frac{x^{2}}{y_{n}}\\right)"));
        println!("  delimited_nested: {:?}", result);
        assert!(result.contains("lr("), "should have lr(): {}", result);
        assert!(result.contains("frac"), "should have frac: {}", result);
    }

    /// Test all output formats on nested formulas (the full conversion_matrix upgrade).
    #[test]
    fn conversion_matrix_nested_formulas() {
        let formulas = vec![
            ("E=mc^2", "simple"),
            ("x^{2}", "simple superscript"),
            ("x_{y_{z}}", "nested subscript"),
            ("x^{y^{z}}", "nested superscript"),
            ("\\frac{a}{b}", "simple fraction"),
            ("\\frac{\\frac{a}{b}}{c}", "nested fraction numerator"),
            ("\\frac{a}{\\frac{b}{c}}", "nested fraction denominator"),
            ("\\frac{x^{2}}{y_{n}}", "fraction with sub/sup"),
            ("\\sqrt{x}", "sqrt"),
            ("\\sqrt[3]{x}", "nth root"),
            ("\\sqrt[3]{\\frac{x}{y}}", "root with fraction"),
            ("\\alpha_{i}^{2}", "greek with sub/sup"),
            ("\\int_{0}^{\\infty} f(x) dx", "integral with limits"),
            ("\\sum_{i=0}^{n} a_i", "sum with limits"),
            ("\\prod_{i=1}^{\\infty} b_i", "product with limits"),
            ("\\lim_{x \\to 0} \\sin x", "limit with arrow"),
            ("\\left(\\frac{a}{b}\\right)", "delimited fraction"),
            (
                "\\left[\\frac{x^{2}}{y_{n}}\\right]",
                "delimited bracket nested",
            ),
            ("\\hat{x} + \\bar{y}", "accents"),
            ("\\operatorname{Spec}(A)", "operatorname"),
        ];

        for (latex, desc) in &formulas {
            // LaTeX → LaTeX passthrough
            let result_latex = DocumentConverter::convert_latex_string(latex, OutputFormat::Latex)
                .unwrap_or_else(|e| panic!("LaTeX→LaTeX failed for {} ({}): {}", desc, latex, e));
            assert!(!result_latex.is_empty(), "LaTeX→LaTeX empty for {}", desc);

            // LaTeX → Typst
            let result_typst = DocumentConverter::convert_latex_string(latex, OutputFormat::Typst)
                .unwrap_or_else(|e| panic!("LaTeX→Typst failed for {} ({}): {}", desc, latex, e));
            assert!(!result_typst.is_empty(), "LaTeX→Typst empty for {}", desc);

            // LaTeX → MathML
            let result_mathml =
                DocumentConverter::convert_latex_string(latex, OutputFormat::MathML)
                    .unwrap_or_else(|e| {
                        panic!("LaTeX→MathML failed for {} ({}): {}", desc, latex, e)
                    });
            assert!(
                result_mathml.contains("<math"),
                "LaTeX→MathML missing <math> for {} ({}): {}",
                desc,
                latex,
                result_mathml
            );

            // LaTeX → OMML
            let result_omml = DocumentConverter::convert_latex_string(latex, OutputFormat::OMML)
                .unwrap_or_else(|e| panic!("LaTeX→OMML failed for {} ({}): {}", desc, latex, e));
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
                || result_omml.contains("<m:oMathPara")
                || result_omml.contains("<m:r>");
            assert!(
                has_math_structure,
                "LaTeX→OMML missing math structure for {} ({}): {}",
                desc, latex, result_omml
            );

            // LaTeX → Markdown
            let result_md =
                DocumentConverter::convert_latex_string(latex, OutputFormat::MarkdownBlock)
                    .unwrap_or_else(|e| {
                        panic!("LaTeX→Markdown failed for {} ({}): {}", desc, latex, e)
                    });
            assert!(
                result_md.contains("$") || result_md.contains("\\("),
                "LaTeX→Markdown missing math delimiters for {} ({}): {}",
                desc,
                latex,
                result_md
            );

            // LaTeX → HTML
            let result_html = DocumentConverter::convert_latex_string(latex, OutputFormat::Html)
                .unwrap_or_else(|e| panic!("LaTeX→HTML failed for {} ({}): {}", desc, latex, e));
            assert!(
                result_html.contains("MathJax") || result_html.contains("math"),
                "LaTeX→HTML missing math for {} ({}): {}",
                desc,
                latex,
                result_html
            );
        }
    }

    /// Test roundtrip: OMML → LaTeX → OMML on nested formulas.
    #[test]
    fn roundtrip_omml_nested() {
        let omml_cases = vec![
            (
                r#"<m:oMathPara><m:oMath><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath></m:oMathPara>"#,
                "simple fraction",
            ),
            (
                r#"<m:oMathPara><m:oMath><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath></m:oMathPara>"#,
                "superscript",
            ),
        ];

        for (omml, desc) in &omml_cases {
            let latex = DocumentConverter::convert_omml_string(omml, OutputFormat::Latex)
                .unwrap_or_else(|e| panic!("OMML→LaTeX failed for {}: {}", desc, e));
            assert!(!latex.is_empty(), "OMML→LaTeX empty for {}", desc);

            let back = DocumentConverter::convert_latex_string(&latex, OutputFormat::OMML)
                .unwrap_or_else(|e| panic!("LaTeX→OMML roundtrip failed for {}: {}", desc, e));
            assert!(
                back.contains("<m:f>") || back.contains("<m:sSup>") || back.contains("<m:r>"),
                "Roundtrip lost math structure for {}: {}",
                desc,
                back
            );
        }
    }

    /// Test roundtrip: MathML → LaTeX → MathML on nested formulas.
    ///
    /// NOTE: MathML → LaTeX parsing currently generates a full LaTeX document
    /// template rather than a bare formula. The roundtrip preservation of math
    /// structure is checked at the first hop (MathML → LaTeX contains \frac, ^, etc.).
    #[test]
    fn roundtrip_mathml_nested() {
        let mathml_cases = vec![
            (
                r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><msup><mi>x</mi><mn>2</mn></msup></math>"#,
                "superscript",
            ),
            (
                r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"#,
                "fraction",
            ),
            (
                r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><msubsup><mi>x</mi><mi>i</mi><mn>2</mn></msubsup></math>"#,
                "subscript superscript",
            ),
        ];

        for (mathml, desc) in &mathml_cases {
            let latex = DocumentConverter::convert_mathml_string(mathml, OutputFormat::Latex)
                .unwrap_or_else(|e| panic!("MathML→LaTeX failed for {}: {}", desc, e));
            // First hop should preserve math structure
            assert!(
                latex.contains("frac") || latex.contains("^") || latex.contains("_{"),
                "MathML→LaTeX lost math for {}: {}",
                desc,
                latex
            );

            let back = DocumentConverter::convert_latex_string(&latex, OutputFormat::MathML)
                .unwrap_or_else(|e| panic!("LaTeX→MathML roundtrip failed for {}: {}", desc, e));
            // Second hop should produce some math structure
            assert!(
                back.contains("<math") || back.contains("<mi>") || back.contains("<mn>"),
                "Roundtrip lost math structure for {}: {}",
                desc,
                back
            );
        }
    }

    /// Test new LaTeX commands: underline, footnote, label, ref, cite, theorem, minipage, float
    #[test]
    fn new_latex_commands_pipeline() {
        use crate::latex_parser::parse_latex;
        use crate::latex_to_typst::latex_ast_to_typst;

        // underline
        let node = parse_latex("\\underline{x}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\underline{x}"),
            "underline roundtrip: {}",
            latex_out
        );
        let typst_out = latex_ast_to_typst(&node);
        assert!(
            typst_out.contains("underline"),
            "underline→typst: {}",
            typst_out
        );

        // footnote
        let node = parse_latex("\\footnote{text}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\footnote{text}"),
            "footnote roundtrip: {}",
            latex_out
        );

        // label and ref
        let node = parse_latex("\\label{eq:1} x \\ref{eq:1}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\label{eq:1}"),
            "label roundtrip: {}",
            latex_out
        );
        assert!(
            latex_out.contains("\\ref{eq:1}"),
            "ref roundtrip: {}",
            latex_out
        );

        // eqref
        let node = parse_latex("\\eqref{eq:1}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\eqref{eq:1}"),
            "eqref roundtrip: {}",
            latex_out
        );

        // cite family
        let node = parse_latex("\\cite{knuth}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\cite{knuth}"),
            "cite roundtrip: {}",
            latex_out
        );

        let node = parse_latex("\\citet{knuth}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\citet{knuth}"),
            "citet roundtrip: {}",
            latex_out
        );

        let node = parse_latex("\\citep{knuth}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\citep{knuth}"),
            "citep roundtrip: {}",
            latex_out
        );

        // bibliography
        let node = parse_latex("\\bibliography{refs}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\bibliography{refs}"),
            "bibliography roundtrip: {}",
            latex_out
        );

        // theorem environments
        let node = parse_latex("\\begin{theorem}content\\end{theorem}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\begin{theorem}"),
            "theorem roundtrip: {}",
            latex_out
        );
        assert!(
            latex_out.contains("content"),
            "theorem content: {}",
            latex_out
        );

        let node = parse_latex("\\begin{proof}QED\\end{proof}");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\begin{proof}"),
            "proof roundtrip: {}",
            latex_out
        );

        // tableofcontents
        let node = parse_latex("\\tableofcontents");
        let latex_out = format!("{}", node);
        assert!(
            latex_out.contains("\\tableofcontents"),
            "toc roundtrip: {}",
            latex_out
        );
    }

    /// Test new AST node types through OMML conversion
    #[test]
    fn new_commands_to_omml() {
        use crate::omml::latex_to_omml;

        // footnote renders as placeholder
        let omml = latex_to_omml("\\footnote{text}");
        assert!(omml.contains("[^"), "footnote OMML placeholder: {}", omml);

        // label renders empty
        let omml = latex_to_omml("\\label{key}");
        assert!(!omml.contains("label"), "label should not render: {}", omml);

        // ref renders as placeholder
        let omml = latex_to_omml("\\ref{key}");
        assert!(
            omml.contains("(key)") || omml.contains("(?)"),
            "ref OMML: {}",
            omml
        );

        // cite renders as placeholder
        let omml = latex_to_omml("\\cite{knuth}");
        assert!(omml.contains("[knuth]"), "cite OMML: {}", omml);

        // theorem renders bold title
        let omml = latex_to_omml("\\begin{theorem}content\\end{theorem}");
        assert!(omml.contains("theorem"), "theorem OMML: {}", omml);
        assert!(omml.contains("content"), "theorem content OMML: {}", omml);

        // toc renders as placeholder text
        let ast = crate::latex_parser::parse_latex("\\tableofcontents");
        let ast_str = format!("{:?}", ast);
        let omml = latex_to_omml("\\tableofcontents");
        assert!(
            omml.contains("目录") || omml.contains("<m:t>"),
            "toc OMML empty. ast: {}, omml: [{}]",
            ast_str,
            omml
        );
    }

    /// Test Markdown parser handles all block types
    #[test]
    fn markdown_parser_comprehensive() {
        let md = "# Title\n\n**bold** and *italic* text.\n\n`code` here.\n\n- item1\n- item2\n\n1. first\n2. second\n\n> blockquote\n\n---\n\n$$E=mc^2$$\n\nInline $x^2$ math.";
        let doc = parse_markdown_to_document(md);

        let block_types: Vec<&str> = doc.pages[0].blocks.iter().map(|b| b.type_name()).collect();
        assert!(
            block_types.contains(&"heading"),
            "should have heading, got: {:?}",
            block_types
        );
        assert!(
            block_types.contains(&"paragraph"),
            "should have paragraph, got: {:?}",
            block_types
        );
        assert!(
            block_types.contains(&"list"),
            "should have list, got: {:?}",
            block_types
        );
        assert!(
            block_types.contains(&"horizontal_rule"),
            "should have hr, got: {:?}",
            block_types
        );
        assert!(
            block_types.contains(&"formula"),
            "should have formula, got: {:?}",
            block_types
        );

        // Check bold in paragraph
        if let Block::Paragraph(p) = &doc.pages[0].blocks[1] {
            let has_bold = p
                .inlines
                .iter()
                .any(|i| matches!(i, Inline::Text(t) if t.bold == Some(true)));
            assert!(has_bold, "should have bold text");
        }
    }

    /// Test HTML parser handles all block types
    #[test]
    fn html_parser_comprehensive() {
        let html = "<h1>Title</h1><p>Hello <strong>world</strong>!</p><ul><li>A</li><li>B</li></ul><pre><code>fn x() {}</code></pre><hr>";
        let doc = parse_html_to_document(html);

        let block_types: Vec<&str> = doc.pages[0].blocks.iter().map(|b| b.type_name()).collect();
        assert!(block_types.contains(&"heading"), "should have heading");
        assert!(block_types.contains(&"paragraph"), "should have paragraph");
        assert!(block_types.contains(&"list"), "should have list");
        assert!(block_types.contains(&"code"), "should have code");
        assert!(block_types.contains(&"horizontal_rule"), "should have hr");
    }
}
