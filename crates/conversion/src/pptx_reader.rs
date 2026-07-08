//! PPTX reader — extracts text, shapes, and images from PowerPoint presentations.
//!
//! Reads .pptx files (ZIP archives containing OOXML) and produces a Document AST
//! with one Page per slide.

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;

/// Parse a .pptx file and produce a Document AST (one Page per slide).
pub fn read_pptx(path: impl AsRef<Path>) -> Result<Document> {
    let file = std::fs::File::open(path.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to open PPTX: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SnipperError::Export(format!("Failed to read PPTX archive: {}", e)))?;

    // Read presentation.xml for slide list
    let pres_xml = read_entry(&mut archive, "ppt/presentation.xml")?;
    let slide_rels = parse_slide_rels(&pres_xml);

    let mut pages = Vec::new();

    for (slide_idx, (slide_file, slide_name)) in slide_rels.iter().enumerate() {
        let slide_xml = match read_entry(&mut archive, slide_file) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let rels_file = format!("ppt/slides/_rels/{}.rels", slide_name);
        let rels_xml = read_entry(&mut archive, &rels_file).unwrap_or_default();
        let rels = parse_rels(&rels_xml);

        let blocks = parse_slide_body(&slide_xml, &mut archive, &rels);

        pages.push(Page {
            width: 960.0,
            height: 540.0,
            blocks,
            page_number: Some((slide_idx + 1) as u32),
        });
    }

    Ok(Document {
        metadata: Metadata {
            language: None, created_at: None,
            ocr_model: Some("pptx".to_string()), ocr_version: Some("1.0".to_string()),
            ocr_time_ms: None,
        },
        pages,
        assets: Vec::new(), diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    })
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut file = archive.by_name(name)
        .map_err(|_| SnipperError::Export(format!("Entry '{}' not found", name)))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| SnipperError::Export(format!("Failed to read '{}': {}", name, e)))?;
    Ok(content)
}

fn parse_slide_rels(pres_xml: &str) -> Vec<(String, String)> {
    let mut slides = Vec::new();
    let mut reader = Reader::from_str(pres_xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"p:sldId" || e.name().as_ref() == b"sldId" {
                    let mut id = String::new();
                    let mut r_id = String::new();
                    for attr in e.attributes().flatten() {
                        let k = attr.key.as_ref().to_vec();
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        if k == b"id" { id = v.clone(); }
                        if k.ends_with(b"id") || k == b"r:id" { r_id = v; }
                    }
                    slides.push((id, r_id));
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // Look up slide files from relationships
    // Fallback: slides are in ppt/slides/slideN.xml
    if slides.is_empty() {
        for i in 1..=50 {
            slides.push((i.to_string(), format!("rId{}", i)));
        }
    }

    slides.iter().map(|(_id, r_id)| {
        format!("ppt/slides/slide{}.xml", r_id.trim_start_matches("rId"))
    }).enumerate().map(|(i, f)| (f, format!("slide{}", i + 1))).collect()
}

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
                        if attr.key.as_ref() == b"Id" { id = String::from_utf8_lossy(&attr.value).to_string(); }
                        if attr.key.as_ref() == b"Target" { target = String::from_utf8_lossy(&attr.value).to_string(); }
                    }
                    if !id.is_empty() { rels.insert(id, target); }
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

fn parse_slide_body(
    xml: &str,
    archive: &mut zip::ZipArchive<std::fs::File>,
    rels: &std::collections::HashMap<String, String>,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_text_body = false;
    let mut in_paragraph = false;
    let mut current_text = String::new();
    let mut in_run = false;
    let mut run_bold = false;
    let mut run_italic = false;
    let mut in_t = false;
    let mut shape_type = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"p:sp" | b"sp" => { shape_type.clear(); blocks.push(Block::Paragraph(ParagraphBlock { inlines: Vec::new(), geometry: None, source: None })); }
                    b"p:txBody" | b"txBody" => in_text_body = true,
                    b"a:p" | b"p" if in_text_body => {
                        in_paragraph = true;
                        current_text.clear();
                    }
                    b"a:r" | b"r" if in_paragraph => {
                        in_run = true;
                        run_bold = false;
                        run_italic = false;
                    }
                    b"a:rPr" | b"rPr" => {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_vec();
                            if k == b"b" || k == b"bold" { run_bold = String::from_utf8_lossy(&attr.value) == "1"; }
                            if k == b"i" || k == b"italic" { run_italic = String::from_utf8_lossy(&attr.value) == "1"; }
                        }
                    }
                    b"a:t" | b"t" if in_run => in_t = true,
                    b"p:pic" | b"pic" => {
                        // Extract image from blipFill
                        let img_id = e.attributes().flatten()
                            .find(|a| a.key.as_ref().ends_with(b"embed"))
                            .and_then(|a| {
                                let id = String::from_utf8_lossy(&a.value).to_string();
                                rels.get(&id).cloned()
                            });
                        if let Some(ref path) = img_id {
                            let media_path = if path.starts_with("../") {
                                format!("ppt/{}", &path[3..])
                            } else {
                                format!("ppt/{}", path)
                            };
                            if let Ok(mut img_file) = archive.by_name(&media_path) {
                                let mut img_bytes = Vec::new();
                                if img_file.read_to_end(&mut img_bytes).is_ok() {
                                    let b64 = base64_encode(&img_bytes);
                                    blocks.push(Block::Paragraph(ParagraphBlock {
                                        inlines: vec![Inline::Image(ImageInline {
                                            asset_id: None, image_data: Some(b64),
                                            width: None, height: None, alt_text: None, source: None,
                                        })],
                                        geometry: None, source: None,
                                    }));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_t {
                    if let Ok(text) = e.unescape() {
                        current_text.push_str(&text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"a:t" | b"t" => in_t = false,
                    b"a:r" | b"r" => in_run = false,
                    b"a:p" | b"p" if in_paragraph => {
                        in_paragraph = false;
                        if !current_text.trim().is_empty() {
                            if let Some(Block::Paragraph(ref mut p)) = blocks.last_mut() {
                                let text = current_text.trim().to_string();
                                let tr = TextRun::new(text)
                                    .with_bold(run_bold)
                                    .with_italic(run_italic);
                                p.inlines.push(Inline::Text(tr));
                            }
                        }
                    }
                    b"p:txBody" | b"txBody" => in_text_body = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    blocks.retain(|b| match b {
        Block::Paragraph(p) => !p.inlines.is_empty(),
        _ => true,
    });
    blocks
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
        if chunk.len() > 1 { result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char); }
        else { result.push('='); }
        if chunk.len() > 2 { result.push(CHARS[(triple & 0x3F) as usize] as char); }
        else { result.push('='); }
    }
    result
}
