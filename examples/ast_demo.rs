//! AST Demo — Build and manipulate Document AST
//!
//! Run: cargo run --example ast_demo

use latexsnipper_ast::*;

fn main() {
    println!("=== LaTeXSnipper AST Demo ===\n");

    // Build a document using the builder API
    let doc = DocumentBuilder::new()
        .page(800.0, 600.0, |page| {
            page.heading(1, "Math Document");
            page.paragraph(|p| {
                p.text("The equation ");
                p.formula("\\frac{a}{b}");
                p.text(" is called a fraction.");
            });
            page.display_formula("\\sum_{i=1}^{n} x_i = \\bar{x}");
            page.unordered_list(|l| {
                l.text_item("Supports inline math");
                l.text_item("Supports display math");
                l.text_item("Supports tables and figures");
            });
            page.code("rust", "fn main() {\n    println!(\"Hello!\");\n}");
            page.quote(|q| {
                q.text_paragraph("Mathematics is the language of the universe.");
                q.with_attribution("Galileo");
            });
        })
        .build();

    println!(
        "Document: {} pages, {} blocks",
        doc.pages.len(),
        doc.block_count()
    );

    // List all blocks
    for (i, block) in doc.all_blocks().iter().enumerate() {
        println!("  [{}] {}", i, block.type_name());
    }

    // Export to JSON
    let json = serde_json::to_string_pretty(&doc).unwrap();
    println!("\nJSON (first 300 chars):");
    println!("{}...", &json[..json.len().min(300)]);

    // Use visitor to collect text
    use latexsnipper_ast::DocumentVisitor;
    let mut collector = latexsnipper_ast::TextCollector::new();
    collector.visit_document(&doc);
    println!("\nExtracted text:");
    println!("{}", collector.text);
}
