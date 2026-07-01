use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

use crate::converter::Converter;
use crate::{
    HtmlConverter, LatexConverter, LatexDisplayConverter, LatexEquationConverter, MathmlConverter,
    MarkdownBlockConverter, MarkdownInlineConverter, OmmlConverter, TypstConverter,
};

/// Supported output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Latex,
    LatexDisplay,
    LatexEquation,
    Typst,
    MarkdownInline,
    MarkdownBlock,
    MathML,
    OMML,
    Html,
}

impl OutputFormat {
    pub fn all() -> &'static [OutputFormat] {
        &[
            OutputFormat::Latex,
            OutputFormat::Typst,
            OutputFormat::MarkdownInline,
            OutputFormat::MarkdownBlock,
            OutputFormat::MathML,
            OutputFormat::OMML,
            OutputFormat::Html,
        ]
    }

    pub fn name(&self) -> &str {
        match self {
            OutputFormat::Latex => "latex",
            OutputFormat::LatexDisplay => "latex_display",
            OutputFormat::LatexEquation => "latex_equation",
            OutputFormat::Typst => "typst",
            OutputFormat::MarkdownInline => "markdown_inline",
            OutputFormat::MarkdownBlock => "markdown_block",
            OutputFormat::MathML => "mathml",
            OutputFormat::OMML => "omml",
            OutputFormat::Html => "html",
        }
    }

    pub fn extension(&self) -> &str {
        match self {
            OutputFormat::Latex | OutputFormat::LatexDisplay | OutputFormat::LatexEquation => "tex",
            OutputFormat::Typst => "typ",
            OutputFormat::MarkdownInline | OutputFormat::MarkdownBlock => "md",
            OutputFormat::MathML | OutputFormat::OMML => "xml",
            OutputFormat::Html => "html",
        }
    }
}

/// Unified converter that can convert Document AST to any supported format.
pub struct DocumentConverter {
    format: OutputFormat,
}

impl DocumentConverter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    pub fn convert(&self, doc: &Document) -> Result<String> {
        let converter: Box<dyn Converter> = match self.format {
            OutputFormat::Latex => Box::new(LatexConverter),
            OutputFormat::LatexDisplay => Box::new(LatexDisplayConverter),
            OutputFormat::LatexEquation => Box::new(LatexEquationConverter),
            OutputFormat::Typst => Box::new(TypstConverter),
            OutputFormat::MarkdownInline => Box::new(MarkdownInlineConverter),
            OutputFormat::MarkdownBlock => Box::new(MarkdownBlockConverter),
            OutputFormat::MathML => Box::new(MathmlConverter),
            OutputFormat::OMML => Box::new(OmmlConverter),
            OutputFormat::Html => Box::new(HtmlConverter),
        };
        converter.convert(doc)
    }

    pub fn convert_all(doc: &Document) -> Result<Vec<(OutputFormat, String)>> {
        let mut results = Vec::new();
        for &format in OutputFormat::all() {
            let converter = DocumentConverter::new(format);
            let output = converter.convert(doc)?;
            results.push((format, output));
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::DocumentBuilder;

    fn test_doc() -> Document {
        DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.heading(1, "Math Document");
                page.paragraph(|p| {
                    p.text("The equation ");
                    p.formula("\\frac{a}{b}");
                    p.text(" is important.");
                });
                page.display_formula("E = mc^2");
                page.unordered_list(|l| {
                    l.text_item("Item 1");
                    l.text_item("Item 2");
                });
            })
            .build()
    }

    #[test]
    fn convert_to_latex() {
        let doc = test_doc();
        let converter = DocumentConverter::new(OutputFormat::Latex);
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("\\section{Math Document}"));
        assert!(result.contains("\\frac{a}{b}"));
    }

    #[test]
    fn convert_to_markdown() {
        let doc = test_doc();
        let converter = DocumentConverter::new(OutputFormat::MarkdownBlock);
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("# Math Document"));
        assert!(result.contains("\\frac{a}{b}"));
        assert!(result.contains("- Item 1"));
    }

    #[test]
    fn convert_to_typst() {
        let doc = test_doc();
        let converter = DocumentConverter::new(OutputFormat::Typst);
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("Math Document"));
        assert!(result.contains("frac(a, b)") || result.contains("(a)/(b)"));
    }

    #[test]
    fn convert_all_formats() {
        let doc = test_doc();
        let results = DocumentConverter::convert_all(&doc).unwrap();
        assert_eq!(results.len(), 7);
        for (format, output) in &results {
            assert!(!output.is_empty(), "Empty output for {:?}", format);
        }
    }
}
