//! SDK Demo — One-line Image to Export
//!
//! Run: cargo run --example sdk_demo

use latexsnipper_pipeline::sdk::Snipper;

fn main() {
    println!("=== LaTeXSnipper SDK Demo ===\n");

    // One line to process an image
    let snipper = Snipper::from_file("fixtures/formula.png")
        .expect("Failed to process image");

    println!("Detected {} formulas\n", snipper.document().block_count());

    // Export to any format
    println!("── LaTeX ──");
    println!("{}\n", snipper.to_latex().unwrap());

    println!("── Markdown ──");
    println!("{}\n", snipper.to_markdown().unwrap());

    println!("── Typst ──");
    println!("{}\n", snipper.to_typst().unwrap());

    println!("── JSON (first 500 chars) ──");
    let json = snipper.to_json().unwrap();
    println!("{}...", &json[..json.len().min(500)]);
}
