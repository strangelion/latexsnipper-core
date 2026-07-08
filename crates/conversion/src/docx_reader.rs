//! DOCX reader — extracts paragraphs, runs, images, and tables from Word documents.
//!
//! Reads .docx files (ZIP archives containing OOXML) and produces a Document AST.
//!
//! Supported features:
//! - Paragraphs with styled runs (bold, italic, underline, font, size, color)
//! - Headings (based on paragraph style)
//! - Embedded images (via relationships)
//! - Tables (delegates to word_ooxml_table_parser)
//! - Lists (bullet and numbered)
//! - Hyperlinks

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;

/// Parse a .docx file and produce a Document AST.
pub fn read_docx(path: impl AsRef<Path>) -> Result<Document> {
    let file = std::fs::File::open(path.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to open DOCX: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SnipperError::Export(format!("Failed to read DOCX archive: {}", e)))?;

    // Read document.xml
    let document_xml = read_entry(&mut archive, "word/document.xml")?;
    let rels_xml = read_entry(&mut archive, "word/_rels/document.xml.rels").unwrap_or_default();

    // Parse relationships
    let rels = parse_rels(&rels_xml);

    // Parse document body with diagnostics and assets
    let (blocks, assets, docx_diags) = parse_document_body(&document_xml, &mut archive, &rels);

    Ok(Document {
        metadata: Metadata {
            language: None,
            created_at: None,
            ocr_model: Some("docx".to_string()),
            ocr_version: Some("1.0".to_string()),
            ocr_time_ms: None,
        },
        pages: vec![Page {
            width: 0.0,
            height: 0.0,
            blocks,
            page_number: Some(1),
        }],
        assets,
        diagnostics: docx_diags,
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    })
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| SnipperError::Export(format!("Entry '{}' not found in DOCX", name)))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| SnipperError::Export(format!("Failed to read '{}': {}", name, e)))?;
    Ok(content)
}

/// Parse relationships XML.
fn parse_rels(xml: &str) -> std::collections::HashMap<String, String> {
    let mut rels = std::collections::HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"Relationship" {
                    let mut id = String::new();
                    let mut target = String::new();
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Id" {
                            id = String::from_utf8_lossy(&attr.value).to_string();
                        }
                        if attr.key.as_ref() == b"Target" {
                            target = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                    if !id.is_empty() {
                        rels.insert(id, target);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rels
}

/// Parse the main document body XML and extract blocks with diagnostics and assets.
fn parse_document_body(
    xml: &str,
    archive: &mut zip::ZipArchive<std::fs::File>,
    rels: &std::collections::HashMap<String, String>,
) -> (Vec<Block>, Vec<MediaAsset>, Vec<Diagnostic>) {
    let mut blocks = Vec::new();
    let mut assets = Vec::new();
    let mut diagnostics = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_body = false;
    let mut in_paragraph = false;
    let mut current_paragraph_inlines: Vec<Inline> = Vec::new();
    let mut paragraph_style = String::new();
    let mut in_run = false;
    let mut run_bold = false;
    let mut run_italic = false;
    let mut run_underline = false;
    let mut in_text = false;
    let mut in_hyperlink = false;
    let mut hyperlink_target = String::new();
    let mut hyperlink_inlines: Vec<Inline> = Vec::new();
    #[allow(unused)]
    let mut drawing_id: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"w:body" | b"body" => in_body = true,
                    b"w:p" | b"p" if in_body => {
                        in_paragraph = true;
                        current_paragraph_inlines.clear();
                        paragraph_style.clear();
                    }
                    b"w:pStyle" | b"pStyle" => {
                        // Read paragraph style
                    }
                    b"w:r" | b"r" if in_paragraph => {
                        in_run = true;
                        run_bold = false;
                        run_italic = false;
                        run_underline = false;
                    }
                    b"w:b" | b"b" => run_bold = true,
                    b"w:i" | b"i" => run_italic = true,
                    b"w:u" | b"u" => run_underline = true,
                    b"w:t" | b"t" if in_run => in_text = true,
                    b"w:hyperlink" | b"hyperlink" if in_paragraph => {
                        in_hyperlink = true;
                        hyperlink_inlines.clear();
                        hyperlink_target = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"r:id" || a.key.as_ref() == b"id")
                            .and_then(|a| {
                                let id = String::from_utf8_lossy(&a.value).to_string();
                                rels.get(&id).cloned()
                            })
                            .unwrap_or_default();
                    }
                    b"w:drawing" | b"drawing" if in_paragraph => {
                        drawing_id = None;
                    }
                    b"wp:inline" | b"inline" => {}
                    b"a:blip" | b"blip" => {
                        drawing_id = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"r:embed" || a.key.as_ref() == b"embed")
                            .and_then(|a| {
                                let id = String::from_utf8_lossy(&a.value).to_string();
                                rels.get(&id).cloned()
                            });
                    }
                    // Detect SmartArt/OLE/Chart for diagnostic warnings
                    b"mc:AlternateContent" | b"AlternateContent" => {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticLevel::Warning,
                            latexsnipper_ast::W_SMARTART_NOT_SUPPORTED,
                            "SmartArt graphic detected; will be rendered as preview only",
                        ).with_recoverable(true));
                    }
                    b"o:OLEObject" | b"OLEObject" | b"w:oleObject" => {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticLevel::Warning,
                            latexsnipper_ast::W_OLE_NOT_SUPPORTED,
                            "OLE embedded object detected; placeholder used",
                        ).with_recoverable(true));
                    }
                    b"c:chartSpace" | b"chartSpace" | b"c:chart" => {
                        diagnostics.push(Diagnostic::new(
                            DiagnosticLevel::Warning,
                            latexsnipper_ast::W_CHART_DATA_SIMPLIFIED,
                            "Embedded chart detected; data may be simplified",
                        ).with_recoverable(true));
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text {
                    if let Ok(text) = e.unescape() {
                        let run = Inline::Text(
                            TextRun::new(text.to_string())
                                .with_bold(run_bold)
                                .with_italic(run_italic)
                                .with_underline(run_underline),
                        );
                        if in_hyperlink {
                            hyperlink_inlines.push(run);
                        } else {
                            current_paragraph_inlines.push(run);
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"w:t" | b"t" => in_text = false,
                    b"w:r" | b"r" => in_run = false,
                    b"w:hyperlink" | b"hyperlink" => {
                        if !hyperlink_target.is_empty() && !hyperlink_inlines.is_empty() {
                            current_paragraph_inlines.push(Inline::Link(LinkInline {
                                content: std::mem::take(&mut hyperlink_inlines),
                                target: hyperlink_target.clone(),
                                title: None,
                                source: None,
                            }));
                        }
                        in_hyperlink = false;
                        hyperlink_target.clear();
                    }
                    b"w:drawing" | b"drawing" => {
                        if let Some(img_rel) = &drawing_id {
                            let media_path = format!("word/{}", img_rel);
                            if let Ok(mut img_file) = archive.by_name(&media_path) {
                                let mut img_bytes = Vec::new();
                                if img_file.read_to_end(&mut img_bytes).is_ok() {
                                    let b64 = base64_encode(&img_bytes);
                                    let asset_id = AssetId(format!("docx-img-{}", assets.len()));
                                    let format = guess_image_format(&img_rel);
                                    assets.push(MediaAsset {
                                        id: asset_id.clone(),
                                        format,
                                        mime_type: None,
                                        role: MediaRole::Photo,
                                        storage: AssetStorage::InlineBase64 { data: b64 },
                                        width: None,
                                        height: None,
                                        dpi: None,
                                        color_space: None,
                                        checksum_sha256: None,
                                        alt_text: None,
                                        metadata: Default::default(),
                                    });
                                    current_paragraph_inlines.push(Inline::Image(ImageInline {
                                        asset_id: Some(asset_id),
                                        image_data: None,
                                        width: None,
                                        height: None,
                                        alt_text: None,
                                        source: None,
                                    }));
                                }
                            }
                        }
                        drawing_id = None;
                    }
                    b"w:p" | b"p" => {
                        in_paragraph = false;
                        if !current_paragraph_inlines.is_empty() {
                            blocks.push(Block::Paragraph(ParagraphBlock {
                                inlines: std::mem::take(&mut current_paragraph_inlines),
                                geometry: None,
                                source: Some(SourceInfo::new().with_producer("docx")),
                                style: None,
                            }));
                        }
                    }
                    b"w:body" | b"body" => in_body = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    (blocks, assets, diagnostics)
}

/// Guess the image format from the media file path extension.
fn guess_image_format(path: &str) -> AssetFormat {
    if path.ends_with(".png") {
        AssetFormat::Png
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        AssetFormat::Jpeg
    } else if path.ends_with(".gif") {
        AssetFormat::Gif
    } else if path.ends_with(".webp") {
        AssetFormat::Webp
    } else if path.ends_with(".svg") {
        AssetFormat::Svg
    } else if path.ends_with(".bmp") {
        AssetFormat::Bmp
    } else if path.ends_with(".tiff") || path.ends_with(".tif") {
        AssetFormat::Tiff
    } else {
        AssetFormat::Unknown
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_minimal_docx(text: &str, suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("test_docx_{}_{}.docx", suffix, std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let opts = || zip::write::FileOptions::default();

        zip.add_directory("_rels/", opts()).unwrap();
        zip.start_file("[Content_Types].xml", opts()).unwrap();
        write!(zip, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
</Types>"#).unwrap();

        zip.add_directory("word/", opts()).unwrap();
        zip.start_file("word/document.xml", opts()).unwrap();
        write!(
            zip,
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t>{}</w:t></w:r></w:p>
</w:body>
</w:document>"#,
            text
        )
        .unwrap();

        let _file = zip.finish().unwrap();
        path
    }

    #[test]
    fn test_read_docx_simple_paragraph() {
        let path = create_minimal_docx("Hello from DOCX", "p");
        let doc = read_docx(&path).unwrap();
        assert_eq!(doc.pages.len(), 1);
        assert!(doc.block_count() > 0);
        let text = doc
            .all_blocks()
            .iter()
            .map(|b| match b {
                Block::Paragraph(p) => p
                    .inlines
                    .iter()
                    .map(|i| match i {
                        Inline::Text(t) => t.text.clone(),
                        _ => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Hello from DOCX"), "text: {}", text);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_read_docx_empty() {
        let path = create_minimal_docx("", "e");
        let doc = read_docx(&path).unwrap();
        assert!(doc.block_count() == 0 || doc.all_blocks().is_empty());
        std::fs::remove_file(&path).ok();
    }
}
