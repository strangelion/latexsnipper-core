//! Round-trip conversion tests: Input Format → Document AST → Output Format

use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};

/// Create a document with formulas in different source formats
fn test_doc_with_latex_formula() -> Document {
    DocumentBuilder::new()
        .page(800.0, 600.0, |page| {
            page.heading(1, "Formula Test");
            page.paragraph(|p| {
                p.text("The equation ");
                p.formula("\\frac{a}{b}");
                p.text(" is a fraction.");
            });
            page.display_formula("E = mc^2");
            page.display_formula("\\int_{0}^{\\infty} e^{-x^2} dx");
        })
        .build()
}

fn test_doc_with_typst_formula() -> Document {
    Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![
                        Inline::Text(TextRun::new("The equation ")),
                        Inline::Formula(Formula {
                            source: FormulaSource::Typst("frac(a, b)".to_string()),
                            display_mode: false,
                            confidence: 0.95,
                            source_info: None,
                        }),
                        Inline::Text(TextRun::new(" is a fraction.")),
                    ],
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: FormulaSource::Typst("E = m c ^ 2".to_string()),
                        display_mode: true,
                        confidence: 0.92,
                        source_info: None,
                    },
                    geometry: None,
                    source: None,
                }),
            ],
            page_number: Some(1),
        }],
        id_gen: NodeIdGenerator::new(),
    }
}

fn test_doc_with_mathml_formula() -> Document {
    Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![
                        Inline::Text(TextRun::new("The equation ")),
                        Inline::Formula(Formula {
                            source: FormulaSource::MathML("<mfrac><mi>a</mi><mi>b</mi></mfrac>".to_string()),
                            display_mode: false,
                            confidence: 0.95,
                            source_info: None,
                        }),
                        Inline::Text(TextRun::new(" is a fraction.")),
                    ],
                    geometry: None,
                    source: None,
                }),
                Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: FormulaSource::MathML("<mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup>".to_string()),
                        display_mode: true,
                        confidence: 0.92,
                        source_info: None,
                    },
                    geometry: None,
                    source: None,
                }),
            ],
            page_number: Some(1),
        }],
        id_gen: NodeIdGenerator::new(),
    }
}

fn test_doc_with_omml_formula() -> Document {
    Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![
                        Inline::Text(TextRun::new("The equation ")),
                        Inline::Formula(Formula {
                            source: FormulaSource::Omml("<m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f>".to_string()),
                            display_mode: false,
                            confidence: 0.95,
                            source_info: None,
                        }),
                        Inline::Text(TextRun::new(" is a fraction.")),
                    ],
                    geometry: None,
                    source: None,
                }),
            ],
            page_number: Some(1),
        }],
        id_gen: NodeIdGenerator::new(),
    }
}

#[test]
fn test_latex_input_to_all_outputs() {
    let doc = test_doc_with_latex_formula();
    
    // LaTeX → AST → LaTeX
    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    assert!(latex.contains("\\frac{a}{b}"), "LaTeX output should contain fraction");
    assert!(latex.contains("E = mc^2"), "LaTeX output should contain formula");
    
    // LaTeX → AST → Markdown
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    assert!(md.contains("$$"), "Markdown should have $$ delimiters");
    assert!(md.contains("frac"), "Markdown should contain fraction");
    
    // LaTeX → AST → Typst
    let typst = DocumentConverter::new(OutputFormat::Typst).convert(&doc).unwrap();
    assert!(typst.contains("Formula Test"), "Typst should contain heading");
    assert!(typst.contains("frac"), "Typst should contain fraction");
    
    // LaTeX → AST → HTML
    let html = DocumentConverter::new(OutputFormat::Html).convert(&doc).unwrap();
    assert!(html.contains("<!DOCTYPE html>"), "HTML should have doctype");
    assert!(html.contains("MathJax"), "HTML should use MathJax");
    
    // LaTeX → AST → JSON (via serde_json directly)
    let json = serde_json::to_string_pretty(&doc).unwrap();
    assert!(json.contains("\"pages\""), "JSON should contain pages");
    
    println!("✓ LaTeX input → all outputs: PASSED");
}

#[test]
fn test_typst_input_to_all_outputs() {
    let doc = test_doc_with_typst_formula();
    
    // Typst → AST → LaTeX
    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    assert!(latex.contains("frac"), "LaTeX should contain fraction");
    
    // Typst → AST → Markdown
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    assert!(md.contains("$$"), "Markdown should have $$ delimiters");
    
    // Typst → AST → Typst
    let typst = DocumentConverter::new(OutputFormat::Typst).convert(&doc).unwrap();
    assert!(typst.contains("frac"), "Typst should contain fraction");
    
    println!("✓ Typst input → all outputs: PASSED");
}

#[test]
fn test_mathml_input_to_all_outputs() {
    let doc = test_doc_with_mathml_formula();
    
    // MathML → AST → LaTeX
    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    assert!(!latex.is_empty(), "LaTeX output should not be empty");
    
    // MathML → AST → Markdown
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    assert!(!md.is_empty(), "Markdown output should not be empty");
    
    // MathML → AST → Typst
    let typst = DocumentConverter::new(OutputFormat::Typst).convert(&doc).unwrap();
    assert!(!typst.is_empty(), "Typst output should not be empty");
    
    println!("✓ MathML input → all outputs: PASSED");
}

#[test]
fn test_omml_input_to_all_outputs() {
    let doc = test_doc_with_omml_formula();
    
    // OMML → AST → LaTeX
    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    assert!(!latex.is_empty(), "LaTeX output should not be empty");
    
    // OMML → AST → Markdown
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    assert!(!md.is_empty(), "Markdown output should not be empty");
    
    // OMML → AST → Typst
    let typst = DocumentConverter::new(OutputFormat::Typst).convert(&doc).unwrap();
    assert!(!typst.is_empty(), "Typst output should not be empty");
    
    println!("✓ OMML input → all outputs: PASSED");
}

#[test]
fn test_all_inputs_to_all_outputs() {
    let docs = vec![
        ("LaTeX", test_doc_with_latex_formula()),
        ("Typst", test_doc_with_typst_formula()),
        ("MathML", test_doc_with_mathml_formula()),
        ("OMML", test_doc_with_omml_formula()),
    ];
    
    let formats = vec![
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::Html, "HTML"),
        (OutputFormat::MathML, "MathML"),
        (OutputFormat::OMML, "OMML"),
    ];
    
    for (input_name, doc) in &docs {
        for (format, format_name) in &formats {
            let result = DocumentConverter::new(*format).convert(doc);
            assert!(result.is_ok(), "{} → {} failed: {:?}", input_name, format_name, result.err());
            assert!(!result.unwrap().is_empty(), "{} → {} produced empty output", input_name, format_name);
        }
    }
    
    println!("✓ All inputs → All outputs: PASSED");
}
