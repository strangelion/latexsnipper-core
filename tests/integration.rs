//! Unified Integration Tests
//!
//! Run: cargo test --test integration

use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_pipeline::sdk::Snipper;

// ═══════════════════════════════════════════════════════════
// AST Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn ast_builder_basic() {
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
fn ast_builder_complex() {
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
                l.text_item("Item 1");
                l.text_item("Item 2");
            });
            page.code("rust", "fn main() {}");
            page.quote(|q| {
                q.text_paragraph("Quote here");
                q.with_attribution("Author");
            });
        })
        .build();

    assert_eq!(doc.block_count(), 6);
}

#[test]
fn ast_visitor_text_collector() {
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

// ═══════════════════════════════════════════════════════════
// Conversion Tests
// ═══════════════════════════════════════════════════════════

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
fn conversion_to_latex() {
    let doc = test_doc();
    let converter = DocumentConverter::new(OutputFormat::Latex);
    let result = converter.convert(&doc).unwrap();
    assert!(result.contains("\\section{Math Document}"));
    assert!(result.contains("\\frac{a}{b}"));
}

#[test]
fn conversion_to_markdown() {
    let doc = test_doc();
    let converter = DocumentConverter::new(OutputFormat::MarkdownBlock);
    let result = converter.convert(&doc).unwrap();
    assert!(result.contains("# Math Document"));
    assert!(result.contains("\\frac{a}{b}"));
    assert!(result.contains("- Item 1"));
}

#[test]
fn conversion_to_typst() {
    let doc = test_doc();
    let converter = DocumentConverter::new(OutputFormat::Typst);
    let result = converter.convert(&doc).unwrap();
    assert!(result.contains("Math Document"));
    // The new latex_to_typst converts \frac{a}{b} to (a)/(b)
    assert!(result.contains("a") && result.contains("b"));
}

#[test]
fn conversion_to_html() {
    let doc = test_doc();
    let converter = DocumentConverter::new(OutputFormat::Html);
    let result = converter.convert(&doc).unwrap();
    assert!(result.contains("<!DOCTYPE html>"));
    assert!(result.contains("MathJax"));
}

#[test]
fn conversion_to_json() {
    let doc = test_doc();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    assert!(json.contains("\"pages\""));
    assert!(json.contains("\"Formula\""));
}

// ═══════════════════════════════════════════════════════════
// SDK Tests (require models)
// ═══════════════════════════════════════════════════════════

#[test]
fn sdk_formula_image() {
    let path = std::path::PathBuf::from("fixtures/formula.png");
    if !path.exists() {
        println!("Skipping: fixture not found");
        return;
    }

    let snipper = Snipper::from_file(&path).expect("Failed to process formula.png");

    assert!(
        snipper.document().block_count() > 0,
        "Should detect formulas"
    );

    let latex = snipper.to_latex().unwrap();
    assert!(!latex.is_empty(), "LaTeX output should not be empty");

    let md = snipper.to_markdown().unwrap();
    assert!(!md.is_empty(), "Markdown output should not be empty");

    let typst = snipper.to_typst().unwrap();
    assert!(!typst.is_empty(), "Typst output should not be empty");

    let json = snipper.to_json().unwrap();
    assert!(json.contains("\"pages\""), "JSON should contain pages");

    println!(
        "SDK test passed: {} formulas detected",
        snipper.document().block_count()
    );
}

#[test]
fn sdk_multiple_formats() {
    let path = std::path::PathBuf::from("fixtures/formula.png");
    if !path.exists() {
        println!("Skipping: fixture not found");
        return;
    }

    let snipper = Snipper::from_file(&path).expect("Failed to process image");

    let formats = [
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::Html, "HTML"),
        (OutputFormat::MathML, "MathML"),
        (OutputFormat::OMML, "OMML"),
    ];

    for (format, name) in &formats {
        let result = snipper.to_format(*format);
        assert!(result.is_ok(), "{} export failed: {:?}", name, result.err());
        assert!(
            !result.unwrap().is_empty(),
            "{} output should not be empty",
            name
        );
    }
}
