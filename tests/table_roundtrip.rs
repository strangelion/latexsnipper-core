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

/// Helper to create a TableCell with default styling and text content.
fn make_cell(text: &str, colspan: u32, rowspan: u32) -> TableCell {
    let mut cell = if text.is_empty() {
        TableCell {
            content: vec![],
            colspan,
            rowspan,
            data_type: None,
            formula: None,
            style: None,
            border_style: None,
            border_width: None,
            border_color: None,
            background: None,
            alignment: None,
            geometry: None,
            source: None,
        }
    } else {
        TableCell {
            content: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun::new(text.to_string()))],
                geometry: None,
                source: None,
                style: None,
            })],
            colspan,
            rowspan,
            data_type: None,
            formula: None,
            style: None,
            border_style: None,
            border_width: None,
            border_color: None,
            background: None,
            alignment: None,
            geometry: None,
            source: None,
        }
    };
    cell.colspan = colspan;
    cell.rowspan = rowspan;
    cell
}

/// Helper to create a simple table.
fn simple_table(rows: usize, cols: usize, prefix: &str) -> TableBlock {
    let mut table_rows = Vec::new();
    for r in 0..rows {
        let mut row_cells = Vec::new();
        for c in 0..cols {
            row_cells.push(make_cell(&format!("{}{}{}", prefix, r + 1, c + 1), 1, 1));
        }
        table_rows.push(TableRow {
            cells: row_cells,
            height: None,
            is_header: false,
        });
    }
    TableBlock {
        rows: table_rows,
        columns: vec![],
        caption: None,
        style: None,
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with merged cells.
fn merged_table() -> TableBlock {
    TableBlock {
        rows: vec![
            TableRow {
                cells: vec![make_cell("AB", 2, 1), make_cell("C", 1, 1)],
                height: None,
                is_header: false,
            },
            TableRow {
                cells: vec![
                    make_cell("D", 1, 1),
                    make_cell("E", 1, 1),
                    make_cell("F", 1, 1),
                ],
                height: None,
                is_header: false,
            },
        ],
        columns: vec![],
        caption: None,
        style: None,
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with formulas.
fn formula_table() -> TableBlock {
    fn formula_cell(text: &str) -> TableCell {
        TableCell {
            content: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Formula(Formula::latex(text))],
                geometry: None,
                source: None,
                style: None,
            })],
            colspan: 1,
            rowspan: 1,
            data_type: None,
            formula: None,
            style: None,
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
            TableRow {
                cells: vec![formula_cell("E=mc^2"), formula_cell(r"\frac{a}{b}")],
                height: None,
                is_header: false,
            },
            TableRow {
                cells: vec![formula_cell(r"\sum x_i"), formula_cell(r"\int f(x) dx")],
                height: None,
                is_header: false,
            },
        ],
        columns: vec![],
        caption: None,
        style: None,
        geometry: None,
        source: None,
    }
}

/// Helper to create a table with empty cells.
fn empty_cells_table() -> TableBlock {
    TableBlock {
        rows: vec![
            TableRow {
                cells: vec![make_cell("", 1, 1), make_cell("B", 1, 1)],
                height: None,
                is_header: false,
            },
            TableRow {
                cells: vec![make_cell("C", 1, 1), make_cell("", 1, 1)],
                height: None,
                is_header: false,
            },
        ],
        columns: vec![],
        caption: None,
        style: None,
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
            notes: Vec::new(),
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
        notes: Vec::new(),
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
        notes: Vec::new(),
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
        notes: Vec::new(),
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
    fn cell_with_style(
        text: &str,
        border_style: Option<latexsnipper_ast::BorderStyle>,
        border_width: Option<u32>,
        border_color: Option<String>,
        background: Option<String>,
        alignment: Option<latexsnipper_ast::CellAlignment>,
    ) -> TableCell {
        TableCell {
            content: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun::new(text.to_string()))],
                geometry: None,
                source: None,
                style: None,
            })],
            colspan: 1,
            rowspan: 1,
            data_type: None,
            formula: None,
            style: None,
            border_style,
            border_width,
            border_color,
            background,
            alignment,
            geometry: None,
            source: None,
        }
    }

    let table = TableBlock {
        rows: vec![TableRow {
            cells: vec![
                cell_with_style(
                    "Bold",
                    Some(latexsnipper_ast::BorderStyle::Solid),
                    Some(2),
                    Some("red".to_string()),
                    Some("#ffff00".to_string()),
                    Some(latexsnipper_ast::CellAlignment::Center),
                ),
                cell_with_style(
                    "Normal",
                    Some(latexsnipper_ast::BorderStyle::Dashed),
                    Some(1),
                    Some("blue".to_string()),
                    None,
                    Some(latexsnipper_ast::CellAlignment::Right),
                ),
            ],
            height: None,
            is_header: false,
        }],
        columns: vec![],
        caption: None,
        style: None,
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
        notes: Vec::new(),
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
