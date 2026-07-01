//! Conversion Demo — Multi-format export from Document AST
//!
//! Run: cargo run --example conversion_demo

use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};

fn main() {
    println!("=== LaTeXSnipper Conversion Demo ===\n");

    // Build a document
    let doc = DocumentBuilder::new()
        .page(800.0, 600.0, |page| {
            page.heading(1, "Math Document");
            page.paragraph(|p| {
                p.text("The equation ");
                p.formula("\\frac{a}{b}");
                p.text(" is a fraction.");
            });
            page.display_formula("\\sum_{i=1}^{n} x_i = \\bar{x}");
            page.unordered_list(|l| {
                l.text_item("Item 1");
                l.text_item("Item 2");
            });
            page.code("rust", "fn main() {}");
        })
        .build();

    // Export to each format
    let formats = [
        (OutputFormat::Latex, "LaTeX"),
        (OutputFormat::MarkdownBlock, "Markdown"),
        (OutputFormat::Typst, "Typst"),
        (OutputFormat::Html, "HTML"),
        (OutputFormat::MathML, "MathML"),
        (OutputFormat::OMML, "OMML"),
    ];

    for (format, name) in &formats {
        let converter = DocumentConverter::new(*format);
        match converter.convert(&doc) {
            Ok(output) => {
                println!("── {} ({} chars) ──", name, output.len());
                for line in output.lines().take(5) {
                    println!("  {}", line);
                }
                if output.lines().count() > 5 {
                    println!("  ... ({} more lines)", output.lines().count() - 5);
                }
                println!();
            }
            Err(e) => println!("{}: Error: {}\n", name, e),
        }
    }
}
