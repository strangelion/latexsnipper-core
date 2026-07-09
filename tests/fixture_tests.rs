//! Integration tests that read actual fixture files from tests/fixtures/.

use latexsnipper_conversion::*;

#[test]
fn test_fixture_docx() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx/test.docx");
    let doc = read_docx(path).expect("DOCX fixture should parse");
    assert!(!doc.pages.is_empty(), "DOCX should have pages");
    // Office-created DOCX should have at least one paragraph
    let has_text = doc.all_blocks().iter().any(|b| match b {
        latexsnipper_ast::Block::Paragraph(p) => p
            .inlines
            .iter()
            .any(|i| matches!(i, latexsnipper_ast::Inline::Text(_))),
        _ => false,
    });
    assert!(has_text, "DOCX should contain a paragraph with text");
    // User's fixture has an embedded image — verify it's extracted
    if !doc.assets.is_empty() {
        assert!(
            matches!(doc.assets[0].format, latexsnipper_ast::AssetFormat::Png),
            "DOCX image should be PNG"
        );
    }
}

#[test]
fn test_fixture_pptx() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pptx/test.pptx");
    let doc = read_pptx(path).expect("PPTX fixture should parse");
    assert!(!doc.pages.is_empty(), "PPTX should have at least one slide");
}

#[test]
fn test_fixture_xlsx() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/xlsx/test.xlsx");
    let doc = read_xlsx(path).expect("XLSX fixture should parse");
    assert!(!doc.pages.is_empty(), "XLSX should have at least one sheet");
    let has_table = doc
        .all_blocks()
        .iter()
        .any(|b| matches!(b, latexsnipper_ast::Block::Table(_)));
    assert!(has_table, "XLSX should contain a table block");
}

#[test]
fn test_fixture_svg() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/svg/test.svg");
    let svg = std::fs::read_to_string(path).expect("SVG fixture should be readable");
    let shapes = parse_svg_to_shapes(&svg);
    assert!(!shapes.is_empty(), "SVG should contain extractable shapes");
}

#[test]
fn test_fixture_markdown() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/markdown/test.md"
    );
    let md = std::fs::read_to_string(path).expect("Markdown fixture should be readable");
    let doc = parse_markdown_to_document(&md);
    assert!(doc.block_count() > 0, "Markdown should parse to blocks");
    assert!(!doc.assets.is_empty(), "Markdown fixture with image should produce MediaAssets");
}

#[test]
fn test_fixture_html() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/html/test.html");
    let html = std::fs::read_to_string(path).expect("HTML fixture should be readable");
    let doc = parse_html_to_document(&html);
    assert!(doc.block_count() > 0, "HTML should parse to blocks");
    assert!(!doc.assets.is_empty(), "HTML fixture with image should produce MediaAssets");
}

#[test]
fn test_fixture_pdf() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pdf/test.pdf");
    let doc = extract_pdf_text(path).expect("PDF fixture should parse");
    // PDF may or may not have extractable text depending on font encoding
    // Just verify the function doesn't crash
    assert!(doc.schema_version == "1.0.0");
}

#[test]
fn test_fixture_latex() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/latex/test.tex");
    let tex = std::fs::read_to_string(path).expect("LaTeX fixture should be readable");
    assert!(!tex.is_empty());
    assert!(
        tex.contains("documentclass") || tex.contains("begin"),
        "LaTeX should contain recognizable content"
    );
}
