//! Generate test fixture files for all supported formats.
//! Run with: cargo run --example generate_fixtures

use std::io::Write;
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

fn write_docx() {
    use zip::write::FileOptions;
    let path = fixtures_dir().join("docx/test.docx");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = || FileOptions::default();

    zip.add_directory("_rels/", opts()).unwrap();
    zip.start_file("[Content_Types].xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
</Types>"#).unwrap();

    zip.add_directory("word/", opts()).unwrap();
    zip.start_file("word/document.xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:rPr><w:b/><w:i/><w:u/></w:rPr><w:t>Hello DOCX</w:t></w:r></w:p>
<w:p><w:r><w:t>Second paragraph with </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t> text.</w:t></w:r></w:p>
</w:body>
</w:document>"#).unwrap();

    zip.finish().unwrap();
    println!("  ✅ DOCX");
}

fn write_pptx() {
    use zip::write::FileOptions;
    let path = fixtures_dir().join("pptx/test.pptx");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = || FileOptions::default();

    zip.start_file("[Content_Types].xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
</Types>"#).unwrap();

    zip.add_directory("ppt/", opts()).unwrap();
    zip.add_directory("ppt/slides/", opts()).unwrap();
    zip.add_directory("ppt/slides/_rels/", opts()).unwrap();

    zip.start_file("ppt/presentation.xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>
</p:presentation>"#).unwrap();

    zip.start_file("ppt/slides/slide1.xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:spTree>
    <p:sp><p:txBody><a:p xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <a:r><a:rPr b="1" i="1"/><a:t>Hello PPTX</a:t></a:r>
    </a:p></p:txBody></p:sp>
  </p:spTree>
</p:sld>"#).unwrap();

    zip.start_file("ppt/slides/_rels/slide1.xml.rels", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).unwrap();

    zip.finish().unwrap();
    println!("  ✅ PPTX");
}

fn write_xlsx() {
    use zip::write::FileOptions;
    let path = fixtures_dir().join("xlsx/test.xlsx");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = || FileOptions::default();

    zip.start_file("[Content_Types].xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
</Types>"#).unwrap();

    zip.add_directory("xl/", opts()).unwrap();
    zip.add_directory("xl/worksheets/", opts()).unwrap();

    zip.start_file("xl/workbook.xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#).unwrap();

    zip.start_file("xl/worksheets/sheet1.xml", opts()).unwrap();
    write!(zip, r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1"><c r="A1" t="inline"><is><t>Name</t></is></c><c r="B1" t="inline"><is><t>Value</t></is></c></row>
    <row r="2"><c r="A2" t="inline"><is><t>Alpha</t></is></c><c r="B2"><v>42</v></c></row>
    <row r="3"><c r="A3" t="inline"><is><t>Beta</t></is></c><c r="B3"><v>3.14</v></c></row>
  </sheetData>
</worksheet>"#).unwrap();

    zip.finish().unwrap();
    println!("  ✅ XLSX");
}

fn write_pdf_with_typst() {
    let typst_src = "#set page(width: 200pt, height: 100pt)\nHello *PDF* from Typst!\n\n$E = m c^2$";
    let typst_file = fixtures_dir().join("pdf/test.typ");
    std::fs::write(&typst_file, typst_src).unwrap();

    let status = std::process::Command::new("typst")
        .args(["compile", &typst_file.to_string_lossy(), &fixtures_dir().join("pdf/test.pdf").to_string_lossy()])
        .status();
    match status {
        Ok(s) if s.success() => {
            std::fs::remove_file(&typst_file).ok();
            println!("  ✅ PDF (via typst)");
        }
        _ => {
            eprintln!("  ⚠ PDF: typst not available, creating minimal PDF with lopdf");
            write_pdf_lopdf();
        }
    }
}

fn write_pdf_lopdf() {
    use lopdf::*;
    let path = fixtures_dir().join("pdf/test.pdf");
    let mut doc = Document::new();
    let mut page_dict = Dictionary::new();
    page_dict.set("Type", Object::Name(b"Page".to_vec()));
    page_dict.set("MediaBox", Object::Array(vec![
        Object::Integer(0), Object::Integer(0),
        Object::Integer(612), Object::Integer(792),
    ]));
    let page_id = doc.add_object(page_dict);
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_dict.set("Count", Object::Integer(1));
    let pages_id = doc.add_object(pages_dict);
    if let Ok(obj) = doc.get_object_mut(page_id) {
        if let Object::Dictionary(ref mut d) = obj {
            d.set("Parent", Object::Reference(pages_id));
        }
    }
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.save(&path).unwrap();
    println!("  ✅ PDF (via lopdf)");
}

fn main() {
    println!("Generating test fixtures...");
    write_svg();
    write_markdown();
    write_html();
    write_latex();
    write_docx();
    write_pptx();
    write_xlsx();
    write_pdf_with_typst();
    println!("\nAll fixtures generated in tests/fixtures/");
}
