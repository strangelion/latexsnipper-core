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

    // Read presentation.xml for slide list and rels for path resolution
    let pres_xml = read_entry(&mut archive, "ppt/presentation.xml")?;
    let pres_rels_xml =
        read_entry(&mut archive, "ppt/_rels/presentation.xml.rels").unwrap_or_default();
    let pres_rels = parse_rels(&pres_rels_xml);
    let slide_rels = parse_slide_rels(&pres_xml, &pres_rels);

    let mut pages = Vec::new();
    let mut all_assets = Vec::new();

    for (slide_idx, (slide_file, slide_name)) in slide_rels.iter().enumerate() {
        let slide_xml = match read_entry(&mut archive, slide_file) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let rels_file = format!("ppt/slides/_rels/{}.rels", slide_name);
        let rels_xml = read_entry(&mut archive, &rels_file).unwrap_or_default();
        let rels = parse_rels(&rels_xml);

        let (blocks, slide_assets) =
            parse_slide_body(&slide_xml, &mut archive, &rels, &mut all_assets.len());

        all_assets.extend(slide_assets);
        pages.push(Page {
            width: 960.0,
            height: 540.0,
            blocks,
            page_number: Some((slide_idx + 1) as u32),
            layout: None,
            background_asset_id: None,
        });
    }

    Ok(Document {
        metadata: Metadata {
            language: None,
            created_at: None,
            ocr_model: Some("pptx".to_string()),
            ocr_version: Some("1.0".to_string()),
            ocr_time_ms: None,
        },
        pages,
        assets: all_assets,
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
    })
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| SnipperError::Export(format!("Entry '{}' not found", name)))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| SnipperError::Export(format!("Failed to read '{}': {}", name, e)))?;
    Ok(content)
}

fn parse_slide_rels(
    pres_xml: &str,
    pres_rels: &std::collections::HashMap<String, String>,
) -> Vec<(String, String)> {
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
                        if k == b"id" {
                            id = v.clone();
                        }
                        if k.ends_with(b"id") || k == b"r:id" {
                            r_id = v;
                        }
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

    // Use relationships to resolve slide files, with fallback
    if !slides.is_empty() {
        let resolved: Vec<(String, String)> = slides
            .iter()
            .map(|(_id, r_id)| {
                // Try to resolve via presentation rels
                if let Some(target) = pres_rels.get(r_id.as_str()) {
                    let slide_path =
                        if target.starts_with("slides/") || target.starts_with("slides\\") {
                            format!("ppt/{}", target)
                        } else if !target.contains('/') {
                            format!("ppt/slides/{}", target)
                        } else {
                            format!("ppt/{}", target)
                        };
                    let name = slide_path
                        .trim_end_matches(".xml")
                        .rsplit('/')
                        .next()
                        .unwrap_or("slide1")
                        .to_string();
                    (slide_path, name)
                } else {
                    let n = r_id.trim_start_matches("rId");
                    (format!("ppt/slides/slide{}.xml", n), format!("slide{}", n))
                }
            })
            .collect();
        return resolved;
    }

    // Fallback: enumerate slide1..slide50
    for i in 1..=50 {
        slides.push((format!("ppt/slides/slide{}.xml", i), format!("slide{}", i)));
    }
    slides
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

fn parse_slide_body(
    xml: &str,
    archive: &mut zip::ZipArchive<std::fs::File>,
    rels: &std::collections::HashMap<String, String>,
    next_asset_id: &mut usize,
) -> (Vec<Block>, Vec<MediaAsset>) {
    let mut blocks = Vec::new();
    let mut slide_assets = Vec::new();
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
    let mut in_table = false;
    let mut in_table_row = false;
    let mut in_table_cell = false;
    let mut table_rows: Vec<Vec<Inline>> = Vec::new();
    let mut current_cell_inlines: Vec<Inline> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"p:sp" | b"sp" => {
                        shape_type.clear();
                        blocks.push(Block::Paragraph(ParagraphBlock {
                            inlines: Vec::new(),
                            geometry: None,
                            source: None,
                            style: None,
                        }));
                    }
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
                            if k == b"b" || k == b"bold" {
                                run_bold = String::from_utf8_lossy(&attr.value) == "1";
                            }
                            if k == b"i" || k == b"italic" {
                                run_italic = String::from_utf8_lossy(&attr.value) == "1";
                            }
                        }
                    }
                    b"a:t" | b"t" if in_run => in_t = true,
                    b"a:tbl" | b"tbl" => {
                        in_table = true;
                        table_rows.clear();
                    }
                    b"a:tr" | b"tr" if in_table => {
                        in_table_row = true;
                        table_rows.push(Vec::new());
                    }
                    b"a:tc" | b"tc" if in_table_row => {
                        in_table_cell = true;
                        current_cell_inlines.clear();
                    }
                    b"p:pic" | b"pic" => {
                        // Extract image from blipFill
                        let img_id = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref().ends_with(b"embed"))
                            .and_then(|a| {
                                let id = String::from_utf8_lossy(&a.value).to_string();
                                rels.get(&id).cloned()
                            });
                        if let Some(ref path) = img_id {
                            let media_path = if let Some(stripped) = path.strip_prefix("../") {
                                format!("ppt/{}", stripped)
                            } else {
                                format!("ppt/{}", path)
                            };
                            if let Ok(mut img_file) = archive.by_name(&media_path) {
                                let mut img_bytes = Vec::new();
                                if img_file.read_to_end(&mut img_bytes).is_ok() {
                                    let b64 = base64_encode(&img_bytes);
                                    let asset_id = AssetId(format!("pptx-img-{}", *next_asset_id));
                                    *next_asset_id += 1;
                                    let format = guess_image_format(&media_path);
                                    slide_assets.push(MediaAsset {
                                        id: asset_id.clone(),
                                        format,
                                        mime_type: None,
                                        role: MediaRole::Photo,
                                        storage: AssetStorage::InlineBase64 { data: b64 },
                                        width: None,
                                        height: None,
                                        dpi: None,
                                        color_space: None,
                                        checksum: None,
                                        alt_text: None,
                                        metadata: Default::default(),
                                    });
                                    blocks.push(Block::Paragraph(ParagraphBlock {
                                        inlines: vec![Inline::Image(ImageInline {
                                            asset_id: Some(asset_id),
                                            image_data: None,
                                            width: None,
                                            height: None,
                                            alt_text: None,
                                            source: None,
                                        })],
                                        geometry: None,
                                        source: None,
                                        style: None,
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
                            let text = current_text.trim().to_string();
                            let tr = TextRun::new(text)
                                .with_bold(run_bold)
                                .with_italic(run_italic);
                            if in_table_cell {
                                current_cell_inlines.push(Inline::Text(tr));
                            } else if let Some(Block::Paragraph(ref mut p)) = blocks.last_mut() {
                                p.inlines.push(Inline::Text(tr));
                            }
                        }
                    }
                    b"p:txBody" | b"txBody" => in_text_body = false,
                    b"a:tc" | b"tc" if in_table_cell => {
                        in_table_cell = false;
                        if let Some(row) = table_rows.last_mut() {
                            row.append(&mut current_cell_inlines);
                        }
                    }
                    b"a:tr" | b"tr" if in_table_row => {
                        in_table_row = false;
                    }
                    b"a:tbl" | b"tbl" if in_table => {
                        in_table = false;
                        if !table_rows.is_empty() {
                            let mut rows: Vec<TableRow> = Vec::new();
                            for row_inlines in &table_rows {
                                let cell = TableCell {
                                    content: vec![Block::Paragraph(ParagraphBlock {
                                        inlines: row_inlines.clone(),
                                        geometry: None,
                                        source: None,
                                        style: None,
                                    })],
                                    colspan: 1,
                                    rowspan: 1,
                                    border_style: None,
                                    border_width: None,
                                    border_color: None,
                                    background: None,
                                    alignment: None,
                                    data_type: None,
                                    formula: None,
                                    style: None,
                                    geometry: None,
                                    source: None,
                                };
                                rows.push(TableRow {
                                    cells: vec![cell],
                                    height: None,
                                    is_header: false,
                                });
                            }
                            blocks.push(Block::Table(TableBlock {
                                rows,
                                columns: Vec::new(),
                                caption: None,
                                style: None,
                                geometry: None,
                                source: None,
                            }));
                        }
                        table_rows.clear();
                    }
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
    (blocks, slide_assets)
}

/// Guess the image format from a PPTX media file path extension.
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

    fn create_minimal_pptx(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("test_pptx_{}_{}.pptx", suffix, std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = || zip::write::FileOptions::default();

        zip.add_directory("_rels/", opts()).unwrap();
        zip.start_file("[Content_Types].xml", opts()).unwrap();
        write!(zip, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
</Types>"#).unwrap();

        zip.add_directory("ppt/", opts()).unwrap();
        zip.add_directory("ppt/slides/", opts()).unwrap();
        zip.add_directory("ppt/slides/_rels/", opts()).unwrap();

        zip.start_file("ppt/presentation.xml", opts()).unwrap();
        write!(
            zip,
            "<?xml version=\"1.0\"?>
<p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">
  <p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst>
</p:presentation>"
        )
        .unwrap();

        zip.start_file("ppt/slides/slide1.xml", opts()).unwrap();
        write!(
            zip,
            "<?xml version=\"1.0\"?>
<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">
  <p:spTree>
    <p:sp><p:txBody><a:p xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">
      <a:r><a:t>Hello from PPTX</a:t></a:r>
    </a:p></p:txBody></p:sp>
  </p:spTree>
</p:sld>"
        )
        .unwrap();

        zip.start_file("ppt/slides/_rels/slide1.xml.rels", opts())
            .unwrap();
        write!(
            zip,
            "<?xml version=\"1.0\"?>
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>"
        )
        .unwrap();

        zip.finish().unwrap();
        path
    }

    #[test]
    fn test_read_pptx_simple() {
        let path = create_minimal_pptx("p");
        let doc = read_pptx(&path).unwrap();
        assert!(!doc.pages.is_empty(), "should have at least one slide");
        std::fs::remove_file(&path).ok();
    }
}
