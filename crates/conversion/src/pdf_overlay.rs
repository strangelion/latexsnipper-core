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

            ops.push(Operation::new(
                "Tf",
                vec![
                    Object::Name(b"Helvetica".to_vec()),
                    Object::Integer(font_size as i64),
                ],
            ));
            ops.push(Operation::new(
                "Td",
                vec![Object::Real(x), Object::Real(pdf_y)],
            ));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    escaped.into_bytes(),
                    lopdf::StringFormat::Literal,
                )],
            ));
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
        #[allow(clippy::collapsible_match)]
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
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.text.clone(),
            Inline::Formula(f) => f.as_latex().to_string(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn get_page_height(pdf: &lopdf::Document, object_id: lopdf::ObjectId) -> Option<f32> {
    let page_obj = pdf.get_object(object_id).ok()?;
    let dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return None,
    };
    if let Ok(media_box) = dict.get(b"MediaBox") {
        if let Ok((_, Object::Array(ref arr))) = pdf.dereference(media_box) {
            if arr.len() >= 4 {
                return Some(arr[3].as_i64().unwrap_or(792) as f32);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_minimal_pdf(path: &std::path::Path) {
        // Create a minimal valid PDF using lopdf
        let mut doc = lopdf::Document::new();
        let mut dict = lopdf::Dictionary::new();
        dict.set("Type", Object::Name(b"Page".to_vec()));
        dict.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(612),
                Object::Integer(792),
            ]),
        );
        let page_id = doc.add_object(dict);

        let mut pages_dict = lopdf::Dictionary::new();
        pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
        pages_dict.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages_dict.set("Count", Object::Integer(1));
        let pages_id = doc.add_object(pages_dict);

        // Update page's parent reference
        if let Ok(obj) = doc.get_object_mut(page_id) {
            if let Object::Dictionary(ref mut d) = obj {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let mut catalog = lopdf::Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc.save(path).unwrap();
    }

    #[test]
    fn test_overlay_pdf_creates_output() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("test_overlay_src_{}.pdf", std::process::id()));
        let out = dir.join(format!("test_overlay_out_{}.pdf", std::process::id()));

        create_minimal_pdf(&src);

        let mut doc = Document::new();
        doc.pages.push(Page {
            width: 612.0,
            height: 792.0,
            blocks: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun::new("Hello PDF Overlay"))],
                geometry: Some(Rect::new(72.0, 700.0, 200.0, 14.0)),
                source: Some(SourceInfo::new().with_page(0)),
            })],
            page_number: Some(1),
        });

        let result = overlay_pdf(&src, &doc, &out);
        assert!(result.is_ok(), "overlay should succeed: {:?}", result);
        assert!(out.exists(), "output file should exist");
        assert!(
            out.metadata().unwrap().len() > 0,
            "output should not be empty"
        );

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&out).ok();
    }
}
