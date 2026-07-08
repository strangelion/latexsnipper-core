//! Table Roundtrip Tests
//!
//! Tests for table conversion across 4 formats:
//! LaTeX, Typst, Markdown, HTML
//!
//! Run: cargo test --test table_roundtrip

use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};

// ============================================================================
// Test Tables
// ============================================================================

/// Helper to create a simple table.
fn simple_table(rows: usize, cols: usize, prefix: &str) -> TableBlock {
    let mut table_rows = Vec::new();
    for r in 0..rows {
        let mut row = Vec::new();
        for c in 0..cols {
            row.push(make_cell(&format!("{}{}{}", prefix, r + 1, c + 1), 1, 1));
        }
        table_rows.push(row);
    }
    TableBlock {
        rows: table_rows,
        geometry: None,
        source: None,
    }
}

/// Helper to create a TableCell with default styling.
fn make_cell(text: &str, colspan: u32, rowspan: u32) -> TableCell {
    TableCell {
        inlines: vec![Inline::Text(TextRun::new(text.to_string()))],
        colspan,
        rowspan,
        border_style: None,
        border_width: None,
        border_color: None,
        background: None,
        alignment: None,
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with merged cells.
fn merged_table() -> TableBlock {
    TableBlock {
        rows: vec![
            vec![make_cell("AB", 2, 1), make_cell("C", 1, 1)],
            vec![
                make_cell("D", 1, 1),
                make_cell("E", 1, 1),
                make_cell("F", 1, 1),
            ],
        ],
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with formulas.
fn formula_table() -> TableBlock {
    fn formula_cell(text: &str) -> TableCell {
        TableCell {
            inlines: vec![Inline::Formula(Formula::latex(text))],
            colspan: 1,
            rowspan: 1,
            border_style: None,
            border_width: None,
            border_color: None,
            background: None,
            alignment: None,
            geometry: None,
            source: None,
        }
    }

    TableBlock {
        rows: vec![
            vec![formula_cell("E=mc^2"), formula_cell(r"\frac{a}{b}")],
            vec![formula_cell(r"\sum x_i"), formula_cell(r"\int f(x) dx")],
        ],
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with empty cells.
fn empty_cells_table() -> TableBlock {
    TableBlock {
        rows: vec![
            vec![
                TableCell {
                    inlines: vec![],
                    colspan: 1,
                    rowspan: 1,
                    border_style: None,
                    border_width: None,
                    border_color: None,
                    background: None,
                    alignment: None,
                    geometry: None,
                    source: None,
                },
                make_cell("B", 1, 1),
            ],
            vec![
                make_cell("C", 1, 1),
                TableCell {
                    inlines: vec![],
                    colspan: 1,
                    rowspan: 1,
                    border_style: None,
                    border_width: None,
                    border_color: None,
                    background: None,
                    alignment: None,
                    geometry: None,
                    source: None,
                },
            ],
        ],
        geometry: None,
        source: None,
    }
}

/// Helper to create a 5x6 table.
fn table_5x6() -> TableBlock {
    simple_table(5, 6, "cell")
}

// ============================================================================
// Test Functions
// ============================================================================

/// Test all tables to all output formats.
#[test]
fn roundtrip_tables_to_all_formats() {
    let tables: Vec<(&str, TableBlock)> = vec![
        ("simple_2x2", simple_table(2, 2, "")),
        ("merged_3x3", merged_table()),
        ("standard_5x6", table_5x6()),
        ("formula_table", formula_table()),
        ("empty_cells", empty_cells_table()),
    ];

    let formats: Vec<(OutputFormat, &str)> = vec![
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Html, "HTML"),
    ];

    let mut failures = Vec::new();

    for (name, table) in &tables {
        // Create document with table
        let doc = Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: 800.0,
                height: 600.0,
                blocks: vec![Block::Table(table.clone())],
                page_number: Some(1),
            }],
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
        };

        for (format, format_name) in &formats {
            let converter = DocumentConverter::new(*format);
            match converter.convert(&doc) {
                Ok(output) => {
                    if output.is_empty() {
                        failures.push(format!("{}: {} produced empty output", name, format_name));
                    } else {
                        println!("✓ {}/{}: {} chars", name, format_name, output.len());
                    }
                }
                Err(e) => {
                    failures.push(format!("{}: {} failed: {}", name, format_name, e));
                }
            }
        }
    }

    if !failures.is_empty() {
        panic!("Table roundtrip failures:\n{}", failures.join("\n"));
    }
}

/// Test 5x6 table specifically.
#[test]
fn roundtrip_5x6_table() {
    let table = table_5x6();
    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Table(table)],
            page_number: Some(1),
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    };

    let formats = [
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Html, "HTML"),
    ];

    for (format, name) in &formats {
        let converter = DocumentConverter::new(*format);
        let result = converter.convert(&doc).unwrap();
        assert!(!result.is_empty(), "{} output should not be empty", name);
        println!("5x6 table → {}: {} chars", name, result.len());
    }
}

/// Test table with formulas.
///
/// Note: Current table conversion only handles text content.
/// Formula support in tables is a known limitation.
#[test]
fn roundtrip_formula_table() {
    let table = formula_table();
    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Table(table)],
            page_number: Some(1),
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    };

    // All formats should produce non-empty output
    let formats = [
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Html, "HTML"),
    ];

    for (format, name) in &formats {
        let converter = DocumentConverter::new(*format);
        let result = converter.convert(&doc).unwrap();
        assert!(!result.is_empty(), "{} output should not be empty", name);
    }
}

/// Test table with merged cells.
#[test]
fn roundtrip_merged_cells() {
    let table = merged_table();
    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Table(table)],
            page_number: Some(1),
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    };

    // HTML output should contain colspan
    let converter = DocumentConverter::new(OutputFormat::Html);
    let html = converter.convert(&doc).unwrap();
    assert!(
        html.contains("colspan"),
        "HTML should contain colspan for merged cells"
    );
}

/// Test table with styling.
#[test]
fn roundtrip_styled_table() {
    let table = TableBlock {
        rows: vec![vec![
            TableCell {
                inlines: vec![Inline::Text(TextRun::new("Bold".to_string()))],
                colspan: 1,
                rowspan: 1,
                border_style: Some(latexsnipper_ast::BorderStyle::Solid),
                border_width: Some(2),
                border_color: Some("red".to_string()),
                background: Some("#ffff00".to_string()),
                alignment: Some(latexsnipper_ast::CellAlignment::Center),
                geometry: None,
                source: None,
            },
            TableCell {
                inlines: vec![Inline::Text(TextRun::new("Normal".to_string()))],
                colspan: 1,
                rowspan: 1,
                border_style: Some(latexsnipper_ast::BorderStyle::Dashed),
                border_width: Some(1),
                border_color: Some("blue".to_string()),
                background: None,
                alignment: Some(latexsnipper_ast::CellAlignment::Right),
                geometry: None,
                source: None,
            },
        ]],
        geometry: None,
        source: None,
    };

    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![Block::Table(table)],
            page_number: Some(1),
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    };

    // HTML output should contain styling
    let converter = DocumentConverter::new(OutputFormat::Html);
    let html = converter.convert(&doc).unwrap();
    assert!(
        html.contains("border: 2px solid red"),
        "HTML should contain border style"
    );
    assert!(
        html.contains("background-color: #ffff00"),
        "HTML should contain background color"
    );
    assert!(
        html.contains("text-align: center"),
        "HTML should contain text alignment"
    );
    assert!(
        html.contains("border: 1px dashed blue"),
        "HTML should contain dashed border"
    );
    assert!(
        html.contains("text-align: right"),
        "HTML should contain right alignment"
    );

    // LaTeX output should contain alignment
    let converter = DocumentConverter::new(OutputFormat::Latex);
    let latex = converter.convert(&doc).unwrap();
    assert!(
        latex.contains("\\begin{tabular}"),
        "LaTeX should contain tabular environment"
    );
}
