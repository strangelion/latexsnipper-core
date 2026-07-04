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
            row.push(TableCell {
                inlines: vec![Inline::Text(TextRun::new(format!("{}{}{}", prefix, r + 1, c + 1)))],
                colspan: 1,
                rowspan: 1,
                geometry: None,
                source: None,
            });
        }
        table_rows.push(row);
    }
    TableBlock {
        rows: table_rows,
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with merged cells.
fn merged_table() -> TableBlock {
    TableBlock {
        rows: vec![
            vec![
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("AB".to_string()))],
                    colspan: 2,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("C".to_string()))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
            ],
            vec![
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("D".to_string()))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("E".to_string()))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("F".to_string()))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
            ],
        ],
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with formulas.
fn formula_table() -> TableBlock {
    TableBlock {
        rows: vec![
            vec![
                TableCell {
                    inlines: vec![Inline::Formula(Formula::latex("E=mc^2"))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![Inline::Formula(Formula::latex(r"\frac{a}{b}"))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
            ],
            vec![
                TableCell {
                    inlines: vec![Inline::Formula(Formula::latex(r"\sum x_i"))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![Inline::Formula(Formula::latex(r"\int f(x) dx"))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
            ],
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
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("B".to_string()))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
            ],
            vec![
                TableCell {
                    inlines: vec![Inline::Text(TextRun::new("C".to_string()))],
                    colspan: 1,
                    rowspan: 1,
                    geometry: None,
                    source: None,
                },
                TableCell {
                    inlines: vec![],
                    colspan: 1,
                    rowspan: 1,
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
            id_gen: NodeIdGenerator::new(),
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
        id_gen: NodeIdGenerator::new(),
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
        id_gen: NodeIdGenerator::new(),
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
        id_gen: NodeIdGenerator::new(),
    };

    // HTML output should contain colspan
    let converter = DocumentConverter::new(OutputFormat::Html);
    let html = converter.convert(&doc).unwrap();
    assert!(html.contains("colspan"), "HTML should contain colspan for merged cells");
}
