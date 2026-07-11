//! Generate test fixture files for all supported formats.
//! Run with: cargo run --example generate_fixtures

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from("tests/fixtures")
}

fn write_svg() {
    let svg = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"100\">
  <rect x=\"10\" y=\"10\" width=\"80\" height=\"40\" fill=\"#4CAF50\" stroke=\"#333\" stroke-width=\"2\"/>
  <circle cx=\"150\" cy=\"50\" r=\"30\" fill=\"#2196F3\" stroke=\"#333\" stroke-width=\"2\"/>
  <text x=\"20\" y=\"80\" font-family=\"serif\" font-size=\"12\">Hello SVG</text>
</svg>";
    std::fs::write(fixtures_dir().join("svg/test.svg"), svg).unwrap();
    println!("  ✅ SVG");
}

fn write_markdown() {
    let md = "# Test Document\n\nHello **bold** and *italic*.\n\n$$E=mc^2$$\n\n- Item 1\n- Item 2\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
    std::fs::write(fixtures_dir().join("markdown/test.md"), md).unwrap();
    println!("  ✅ Markdown");
}

fn write_html() {
    let html = r#"<!DOCTYPE html><html><body>
<h1>Test</h1>
<p>Hello <strong>world</strong>!</p>
$$E=mc^2$$
</body></html>"#;
    std::fs::write(fixtures_dir().join("html/test.html"), html).unwrap();
    println!("  ✅ HTML");
}

fn write_latex() {
    let tex = r"\documentclass{article}
\usepackage{amsmath}
\begin{document}
\section{Test}
Hello world. $$E=mc^2$$
\end{document}";
    std::fs::write(fixtures_dir().join("latex/test.tex"), tex).unwrap();
    println!("  ✅ LaTeX");
}

fn main() {
    println!("Generating text-based test fixtures...");
    write_svg();
    write_markdown();
    write_html();
    write_latex();
    // Office (DOCX/PPTX/XLSX) and PDF fixtures are created manually to match
    // real Office app output. See tests/fixtures/<format>/ for the actual files.
    // To regenerate: open Office app, create file, save to fixtures dir.
    println!("\nText fixtures generated. Office/PDF fixtures are in tests/fixtures/");
}
