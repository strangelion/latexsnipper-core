use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use std::path::Path;

/// Extract text content from a PDF file, producing a Document AST.
pub fn extract_pdf_text(path: impl AsRef<Path>) -> Result<Document> {
    let pdf = lopdf::Document::load(path.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to load PDF: {}", e)))?;

    let mut pages_out = Vec::new();

    for (&page_num, &object_id) in pdf.get_pages().iter() {
        let page_idx = (page_num as usize).saturating_sub(1);
        let page_size = get_page_size(&pdf, object_id).unwrap_or((612.0, 792.0));

        let text_fragments = extract_page_text(&pdf, object_id)?;

        let blocks: Vec<Block> = text_fragments
            .into_iter()
            .filter(|f| !f.text.trim().is_empty())
            .map(|f| {
                Block::Paragraph(ParagraphBlock {
                    inlines: vec![Inline::Text(TextRun::new(f.text))],
                    geometry: Some(Rect::new(f.x, f.y, f.width, f.height)),
                    source: Some(
                        SourceInfo::new()
                            .with_page(page_idx)
                            .with_confidence(1.0)
                            .with_region(Rect::new(f.x, f.y, f.width, f.height)),
                    ),
                    style: None,
                })
            })
            .collect();

        pages_out.push(Page {
            width: page_size.0,
            height: page_size.1,
            blocks,
            page_number: Some(page_num),
        });
    }

    Ok(Document {
        metadata: Metadata::default(),
        pages: pages_out,
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
    })
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TextFragment {
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font_size: f32, // kept for potential future use (font metric estimation)
}

fn get_page_size(pdf: &lopdf::Document, object_id: lopdf::ObjectId) -> Option<(f32, f32)> {
    let page_dict = pdf.get_object(object_id).ok()?;
    let dict = match page_dict {
        lopdf::Object::Dictionary(ref d) => d,
        _ => return None,
    };
    if let Ok(media_box) = dict.get(b"MediaBox") {
        if let Ok((_ref, lopdf::Object::Array(ref arr))) = pdf.dereference(media_box) {
            if arr.len() >= 4 {
                let x2 = arr[2].as_i64().unwrap_or(612) as f32;
                let y2 = arr[3].as_i64().unwrap_or(792) as f32;
                return Some((x2, y2));
            }
        }
    }
    None
}

fn extract_page_text(
    pdf: &lopdf::Document,
    object_id: lopdf::ObjectId,
) -> Result<Vec<TextFragment>> {
    let mut fragments = Vec::new();
    let mut pos_x = 0.0f32;
    let mut pos_y = 0.0f32;
    let mut font_size = 12.0f32;

    let content = match pdf.get_page_content(object_id) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()),
    };

    let operations = lopdf::content::Content::decode(&content)
        .map_err(|e| SnipperError::Export(format!("Failed to decode PDF content: {}", e)))?;

    for op in &operations.operations {
        let operands = &op.operands;
        match op.operator.as_ref() {
            "Td" | "TD" => {
                if operands.len() >= 2 {
                    pos_x += operands[0].as_i64().unwrap_or(0) as f32;
                    pos_y -= operands[1].as_i64().unwrap_or(0) as f32;
                }
            }
            "cm" => {
                if operands.len() >= 6 {
                    pos_x = operands[4].as_i64().unwrap_or(0) as f32;
                    pos_y = (-operands[5].as_i64().unwrap_or(0)) as f32;
                }
            }
            "Tf" => {
                if operands.len() >= 2 {
                    font_size = operands[1].as_i64().unwrap_or(12) as f32;
                }
            }
            "Tm" => {
                if operands.len() >= 6 {
                    pos_x = operands[4].as_i64().unwrap_or(0) as f32;
                    pos_y = operands[5].as_i64().unwrap_or(0) as f32;
                }
            }
            "T*" => {
                pos_y -= font_size * 1.2;
            }
            "Tj" => {
                if let Some(text) = operands.first().and_then(extract_text_obj) {
                    let tw = estimate_text_width(&text, font_size);
                    fragments.push(TextFragment {
                        text,
                        x: pos_x,
                        y: pos_y,
                        width: tw,
                        height: font_size * 1.2,
                        font_size,
                    });
                }
            }
            "'" => {
                pos_y -= font_size * 1.2;
                if let Some(text) = operands.first().and_then(extract_text_obj) {
                    let tw = estimate_text_width(&text, font_size);
                    fragments.push(TextFragment {
                        text,
                        x: pos_x,
                        y: pos_y,
                        width: tw,
                        height: font_size * 1.2,
                        font_size,
                    });
                }
            }
            "\"" => {
                if operands.len() >= 3 {
                    font_size = operands[2].as_i64().unwrap_or(12) as f32;
                }
                pos_y -= font_size * 1.2;
                if let Some(text) = operands.get(2).and_then(extract_text_obj) {
                    let tw = estimate_text_width(&text, font_size);
                    fragments.push(TextFragment {
                        text,
                        x: pos_x,
                        y: pos_y,
                        width: tw,
                        height: font_size * 1.2,
                        font_size,
                    });
                }
            }
            "TJ" => {
                let text = extract_tj_text(op);
                if !text.is_empty() {
                    let tw = estimate_text_width(&text, font_size);
                    fragments.push(TextFragment {
                        text,
                        x: pos_x,
                        y: pos_y,
                        width: tw,
                        height: font_size * 1.2,
                        font_size,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(fragments)
}

fn extract_text_obj(obj: &lopdf::Object) -> Option<String> {
    match obj {
        lopdf::Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

fn extract_tj_text(op: &lopdf::content::Operation) -> String {
    let mut text = String::new();
    if let Some(arr) = op.operands.first().and_then(|o| o.as_array().ok()) {
        for item in arr.iter() {
            if let Some(s) = extract_text_obj(item) {
                text.push_str(&s);
            }
        }
    }
    text
}

fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.5
}
