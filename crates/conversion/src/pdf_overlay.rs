//! PDF overlay export — overlay Document AST text onto an existing PDF.

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, Stream};
use std::path::Path;

/// Overlay Document AST text onto a source PDF, writing to an output path.
pub fn overlay_pdf(
    source_pdf: impl AsRef<Path>,
    doc: &Document,
    output_pdf: impl AsRef<Path>,
) -> Result<()> {
    let mut pdf = lopdf::Document::load(source_pdf.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to load source PDF: {}", e)))?;

    let pages = pdf.get_pages();

    for (&page_num, &object_id) in pages.iter() {
        let page_idx = (page_num as usize).saturating_sub(1);

        let page_blocks: Vec<&Block> = if page_idx < doc.pages.len() {
            doc.pages[page_idx].blocks.iter().collect()
        } else {
            continue;
        };

        if page_blocks.is_empty() {
            continue;
        }

        let page_height = get_page_height(&pdf, object_id).unwrap_or(792.0);
        let mut ops = Vec::new();
        ops.push(Operation::new("BT", vec![]));

        for block in &page_blocks {
            let (text, x, y, font_size) = match block {
                Block::Paragraph(p) => {
                    let text = collect_text(&p.inlines);
                    let geom = p.geometry.unwrap_or(Rect::new(0.0, 0.0, 100.0, 12.0));
                    (text, geom.x, geom.y, 12.0f32)
                }
                Block::Heading(h) => {
                    let text = collect_text(&h.inlines);
                    let geom = h.geometry.unwrap_or(Rect::new(0.0, 0.0, 100.0, 16.0));
                    (text, geom.x, geom.y, 16.0f32)
                }
                Block::Formula(f) => {
                    let text = f.formula.as_latex().to_string();
                    let geom = f.geometry.unwrap_or(Rect::new(0.0, 0.0, 100.0, 14.0));
                    (text, geom.x, geom.y, 12.0f32)
                }
                _ => continue,
            };

            if text.is_empty() {
                continue;
            }

            let pdf_y = page_height - y - font_size;
            let escaped = escape_pdf_string(&text);

            ops.push(Operation::new("Tf", vec![
                Object::Name(b"Helvetica".to_vec()),
                Object::Integer(font_size as i64),
            ]));
            ops.push(Operation::new("Td", vec![
                Object::Real(x),
                Object::Real(pdf_y),
            ]));
            ops.push(Operation::new("Tj", vec![
                Object::String(escaped.into_bytes(), lopdf::StringFormat::Literal),
            ]));
        }

        ops.push(Operation::new("ET", vec![]));

        // Encode and set as page content
        let content = Content { operations: ops };
        let encoded = Content::encode(&content)
            .map_err(|e| SnipperError::Export(format!("Failed to encode PDF content: {}", e)))?;
        let mut dict = Dictionary::new();
        dict.set("Length", Object::Integer(encoded.len() as i64));
        let stream_obj = Object::Stream(Stream::new(dict, encoded));

        // Get the page object and set its Contents
        if let Ok(page_obj) = pdf.get_object_mut(object_id) {
            if let Object::Dictionary(ref mut page_dict) = page_obj {
                page_dict.set("Contents", stream_obj);
            }
        }
    }

    pdf.save(output_pdf.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to save output PDF: {}", e)))?;

    Ok(())
}

fn collect_text(inlines: &[Inline]) -> String {
    inlines.iter().map(|i| match i {
        Inline::Text(t) => t.text.clone(),
        Inline::Formula(f) => f.as_latex().to_string(),
        _ => String::new(),
    }).collect::<Vec<_>>().join(" ")
}

fn get_page_height(pdf: &lopdf::Document, object_id: lopdf::ObjectId) -> Option<f32> {
    let page_obj = pdf.get_object(object_id).ok()?;
    let dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return None,
    };
    if let Ok(media_box) = dict.get(b"MediaBox") {
        if let Ok((_, resolved)) = pdf.dereference(media_box) {
            if let Object::Array(ref arr) = resolved {
                if arr.len() >= 4 {
                    return Some(arr[3].as_i64().unwrap_or(792) as f32);
                }
            }
        }
    }
    None
}

fn escape_pdf_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
