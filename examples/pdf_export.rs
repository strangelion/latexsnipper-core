use latexsnipper_ast::{
    Block, CodeBlock, Document, Formula, FormulaBlock, FormulaSource, HeadingBlock, Inline,
    ListBlock, ListItem, NodeIdGenerator, Page, ParagraphBlock, TableBlock, TableCell, TextRun,
};
use latexsnipper_export::generator::Generator;
use latexsnipper_export::{PdfGenerator, RenderTree};

fn main() {
    // Create a test document with various block types
    let doc = Document {
        metadata: latexsnipper_ast::Metadata::default(),
        pages: vec![
            Page {
                width: 800.0,
                height: 600.0,
                blocks: vec![
                    Block::Heading(HeadingBlock {
                        level: 1,
                        inlines: vec![Inline::Text(TextRun::new("Test Document"))],
                        id: None,
                        geometry: None,
                        source: None,
                    }),
                    Block::Paragraph(ParagraphBlock {
                        inlines: vec![
                            Inline::Text(TextRun::new("This is a test with ")),
                            Inline::Formula(Formula {
                                source: FormulaSource::Latex("E = mc^2".to_string()),
                                display_mode: false,
                                confidence: 1.0,
                                source_info: None,
                            }),
                            Inline::Text(TextRun::new(" inline.")),
                        ],
                        geometry: None,
                        source: None,
                    }),
                    Block::Formula(FormulaBlock {
                        formula: Formula {
                            source: FormulaSource::Latex("\\frac{a+b}{c}".to_string()),
                            display_mode: true,
                            confidence: 1.0,
                            source_info: None,
                        },
                        geometry: None,
                        source: None,
                    }),
                    Block::Table(TableBlock {
                        rows: vec![
                            vec![
                                TableCell {
                                    inlines: vec![Inline::Text(TextRun::new("Name"))],
                                    colspan: 1,
                                    rowspan: 1,
                                    geometry: None,
                                    source: None,
                                },
                                TableCell {
                                    inlines: vec![Inline::Text(TextRun::new("Value"))],
                                    colspan: 1,
                                    rowspan: 1,
                                    geometry: None,
                                    source: None,
                                },
                            ],
                            vec![
                                TableCell {
                                    inlines: vec![Inline::Text(TextRun::new("Alpha"))],
                                    colspan: 1,
                                    rowspan: 1,
                                    geometry: None,
                                    source: None,
                                },
                                TableCell {
                                    inlines: vec![Inline::Text(TextRun::new("1.0"))],
                                    colspan: 1,
                                    rowspan: 1,
                                    geometry: None,
                                    source: None,
                                },
                            ],
                        ],
                        geometry: None,
                        source: None,
                    }),
                    Block::List(ListBlock {
                        ordered: false,
                        items: vec![
                            ListItem {
                                inlines: vec![Inline::Text(TextRun::new("Item 1"))],
                                checked: None,
                                source: None,
                            },
                            ListItem {
                                inlines: vec![Inline::Text(TextRun::new("Item 2"))],
                                checked: None,
                                source: None,
                            },
                        ],
                        geometry: None,
                        source: None,
                    }),
                    Block::Code(CodeBlock {
                        language: Some("rust".to_string()),
                        code: "fn main() {\n    println!(\"Hello\");\n}".to_string(),
                        geometry: None,
                        source: None,
                    }),
                ],
                page_number: Some(1),
            },
            Page {
                width: 800.0,
                height: 600.0,
                blocks: vec![
                    Block::Heading(HeadingBlock {
                        level: 2,
                        inlines: vec![Inline::Text(TextRun::new("Page 2"))],
                        id: None,
                        geometry: None,
                        source: None,
                    }),
                    Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun::new("This is page 2 content."))],
                        geometry: None,
                        source: None,
                    }),
                ],
                page_number: Some(2),
            },
        ],
        id_gen: NodeIdGenerator::new(),
    };

    // Build render tree
    let tree = RenderTree::from_document(&doc);
    println!("RenderTree built: {} pages", tree.page_count());

    // Generate PDF
    let generator = PdfGenerator;
    match generator.generate(&tree) {
        Ok(pdf_bytes) => {
            let output_path = "test_rust_export.pdf";
            std::fs::write(output_path, pdf_bytes.as_bytes()).expect("Failed to write PDF");
            println!("PDF generated: {}", output_path);

            // Check file size
            let metadata = std::fs::metadata(output_path).expect("Failed to get metadata");
            println!("File size: {} bytes", metadata.len());
        }
        Err(e) => {
            eprintln!("Failed to generate PDF: {}", e);
        }
    }

    // Test page filtering
    let filtered = doc.filter_pages(&[0]);
    println!("\nFiltered document: {} pages", filtered.pages.len());

    let filtered_tree = RenderTree::from_document(&filtered);
    println!("Filtered RenderTree: {} pages", filtered_tree.page_count());

    // Test page range parsing
    let range = Document::parse_page_range("1-3,5,8-10");
    println!("\nParsed page range '1-3,5,8-10': {:?}", range);
}
