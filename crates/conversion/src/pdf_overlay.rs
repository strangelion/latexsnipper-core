//! PDF overlay export — overlay Document AST text onto an existing PDF.

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Object, Stream};
use std::path::Path;

/// PDF overlay rendering options.
#[derive(Debug, Clone, Copy)]
pub struct PdfOverlayOptions {
    /// When false, text remains selectable but is not painted (PDF Tr=3).
    pub visible: bool,
}

impl Default for PdfOverlayOptions {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Overlay Document AST text onto a source PDF, writing to an output path.
pub fn overlay_pdf(
    source_pdf: impl AsRef<Path>,
    doc: &Document,
    output_pdf: impl AsRef<Path>,
) -> Result<()> {
    overlay_pdf_with_options(source_pdf, doc, output_pdf, PdfOverlayOptions::default())
}

/// Overlay Document AST text while preserving all existing page content.
pub fn overlay_pdf_with_options(
    source_pdf: impl AsRef<Path>,
    doc: &Document,
    output_pdf: impl AsRef<Path>,
    options: PdfOverlayOptions,
) -> Result<()> {
    let mut pdf = lopdf::Document::load(source_pdf.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to load source PDF: {}", e)))?;
    if pdf.is_encrypted() {
        return Err(SnipperError::EncryptedFile(
            "PDF overlay source".to_string(),
        ));
    }

    let font_id = pdf.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });

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

        let page_box = get_page_box(&pdf, object_id).unwrap_or((0.0, 0.0, 612.0, 792.0));
        let rotation = get_page_rotation(&pdf, object_id);
        let mut ops = Vec::new();
        ops.push(Operation::new("q", vec![]));
        if let Some(transform) = rotation_transform(page_box, rotation) {
            ops.push(Operation::new("cm", transform));
        }
        ops.push(Operation::new("BT", vec![]));
        if !options.visible {
            ops.push(Operation::new("Tr", vec![Object::Integer(3)]));
        }

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

            let pdf_y = page_box.3 - y - font_size;
            let escaped = escape_pdf_string(&text);

            ops.push(Operation::new(
                "Tf",
                vec![
                    Object::Name(b"F_LatexSnipper".to_vec()),
                    Object::Integer(font_size as i64),
                ],
            ));
            ops.push(Operation::new(
                "Tm",
                vec![
                    Object::Integer(1),
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(1),
                    Object::Real(page_box.0 + x),
                    Object::Real(pdf_y),
                ],
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
        ops.push(Operation::new("Q", vec![]));

        // Encode and set as page content
        let content = Content { operations: ops };
        let encoded = Content::encode(&content)
            .map_err(|e| SnipperError::Export(format!("Failed to encode PDF content: {}", e)))?;
        let mut dict = Dictionary::new();
        dict.set("Length", Object::Integer(encoded.len() as i64));
        let overlay_id = pdf.add_object(Stream::new(dict, encoded));
        append_page_content(&mut pdf, object_id, overlay_id)?;
        add_page_font_resource(&mut pdf, object_id, font_id)?;
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

fn get_page_box(pdf: &lopdf::Document, object_id: lopdf::ObjectId) -> Option<(f32, f32, f32, f32)> {
    let page_obj = pdf.get_object(object_id).ok()?;
    let dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return None,
    };
    for key in [b"CropBox".as_slice(), b"MediaBox".as_slice()] {
        if let Ok(media_box) = dict.get(key) {
            if let Ok((_, Object::Array(ref arr))) = pdf.dereference(media_box) {
                if arr.len() >= 4 {
                    return Some((
                        object_number(&arr[0]).unwrap_or(0.0),
                        object_number(&arr[1]).unwrap_or(0.0),
                        object_number(&arr[2]).unwrap_or(612.0),
                        object_number(&arr[3]).unwrap_or(792.0),
                    ));
                }
            }
        }
    }
    None
}

fn get_page_rotation(pdf: &lopdf::Document, object_id: lopdf::ObjectId) -> i64 {
    pdf.get_object(object_id)
        .ok()
        .and_then(|object| object.as_dict().ok())
        .and_then(|dict| dict.get(b"Rotate").ok())
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360)
}

fn object_number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

fn rotation_transform(page_box: (f32, f32, f32, f32), rotation: i64) -> Option<Vec<Object>> {
    let width = page_box.2 - page_box.0;
    let height = page_box.3 - page_box.1;
    match rotation {
        90 => Some(vec![
            0.into(),
            1.into(),
            (-1).into(),
            0.into(),
            height.into(),
            0.into(),
        ]),
        180 => Some(vec![
            (-1).into(),
            0.into(),
            0.into(),
            (-1).into(),
            width.into(),
            height.into(),
        ]),
        270 => Some(vec![
            0.into(),
            (-1).into(),
            1.into(),
            0.into(),
            0.into(),
            width.into(),
        ]),
        _ => None,
    }
}

fn append_page_content(
    pdf: &mut lopdf::Document,
    page_id: lopdf::ObjectId,
    overlay_id: lopdf::ObjectId,
) -> Result<()> {
    let existing = pdf
        .get_object(page_id)
        .map_err(|error| SnipperError::Export(error.to_string()))?
        .as_dict()
        .map_err(|error| SnipperError::Export(error.to_string()))?
        .get(b"Contents")
        .ok()
        .cloned();
    let contents = match existing {
        Some(Object::Array(mut values)) => {
            values.push(Object::Reference(overlay_id));
            Object::Array(values)
        }
        Some(value) => Object::Array(vec![value, Object::Reference(overlay_id)]),
        None => Object::Reference(overlay_id),
    };
    pdf.get_object_mut(page_id)
        .map_err(|error| SnipperError::Export(error.to_string()))?
        .as_dict_mut()
        .map_err(|error| SnipperError::Export(error.to_string()))?
        .set("Contents", contents);
    Ok(())
}

fn add_page_font_resource(
    pdf: &mut lopdf::Document,
    page_id: lopdf::ObjectId,
    font_id: lopdf::ObjectId,
) -> Result<()> {
    let resource_object = pdf
        .get_object(page_id)
        .ok()
        .and_then(|object| object.as_dict().ok())
        .and_then(|dict| dict.get(b"Resources").ok())
        .cloned();
    let mut resources = resource_object
        .as_ref()
        .and_then(|object| pdf.dereference(object).ok())
        .and_then(|(_, object)| object.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let font_object = resources.get(b"Font").ok().cloned();
    let mut fonts = font_object
        .as_ref()
        .and_then(|object| pdf.dereference(object).ok())
        .and_then(|(_, object)| object.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    fonts.set("F_LatexSnipper", Object::Reference(font_id));
    resources.set("Font", Object::Dictionary(fonts));
    pdf.get_object_mut(page_id)
        .map_err(|error| SnipperError::Export(error.to_string()))?
        .as_dict_mut()
        .map_err(|error| SnipperError::Export(error.to_string()))?
        .set("Resources", Object::Dictionary(resources));
    Ok(())
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
        let original_stream = doc.add_object(Stream::new(Dictionary::new(), b"q\nQ\n".to_vec()));
        dict.set("Contents", Object::Reference(original_stream));
        dict.set(
            "Resources",
            Object::Dictionary(dictionary! {
                "ExtGState" => Object::Dictionary(dictionary! {
                    "Existing" => Object::Dictionary(Dictionary::new()),
                }),
            }),
        );
        dict.set(
            "Annots",
            Object::Array(vec![Object::Dictionary(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Link",
                "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            })]),
        );
        let page_id = doc.add_object(dict);

        let mut pages_dict = lopdf::Dictionary::new();
        pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
        pages_dict.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages_dict.set("Count", Object::Integer(1));
        let pages_id = doc.add_object(pages_dict);

        // Update page's parent reference
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let mut catalog = lopdf::Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let info_id =
            doc.add_object(dictionary! { "Producer" => Object::string_literal("Original") });
        doc.trailer.set("Info", Object::Reference(info_id));

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
                style: None,
            })],
            page_number: Some(1),
            layout: None,
            background_asset_id: None,
        });

        let result = overlay_pdf(&src, &doc, &out);
        assert!(result.is_ok(), "overlay should succeed: {:?}", result);
        assert!(out.exists(), "output file should exist");
        assert!(
            out.metadata().unwrap().len() > 0,
            "output should not be empty"
        );

        let original = lopdf::Document::load(&src).unwrap();
        let overlaid = lopdf::Document::load(&out).unwrap();
        assert_eq!(overlaid.get_pages().len(), original.get_pages().len());
        assert!(overlaid.trailer.get(b"Info").is_ok());
        let page_id = *overlaid.get_pages().values().next().unwrap();
        let page = overlaid.get_object(page_id).unwrap().as_dict().unwrap();
        let contents = page.get(b"Contents").unwrap().as_array().unwrap();
        assert_eq!(
            contents.len(),
            2,
            "original and overlay streams must coexist"
        );
        assert!(page.get(b"Annots").is_ok(), "annotations must be preserved");
        let resources = page.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(resources.get(b"ExtGState").is_ok());
        assert!(resources
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"F_LatexSnipper")
            .is_ok());
        let combined = overlaid.get_page_content(page_id);
        assert!(combined.windows(3).any(|window| window == b"q\nQ"));
        assert!(combined
            .windows(b"Hello PDF Overlay".len())
            .any(|window| window == b"Hello PDF Overlay"));

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&out).ok();
    }

    #[test]
    fn transparent_overlay_uses_invisible_selectable_text_mode() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!(
            "test_overlay_hidden_src_{}.pdf",
            std::process::id()
        ));
        let out = dir.join(format!(
            "test_overlay_hidden_out_{}.pdf",
            std::process::id()
        ));
        create_minimal_pdf(&src);
        let mut document = Document::new();
        let mut page = Page::new(612.0, 792.0, 1);
        page.blocks.push(Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun::new("Selectable OCR"))],
            geometry: Some(Rect::new(10.0, 10.0, 100.0, 12.0)),
            source: None,
            style: None,
        }));
        document.add_page(page);
        overlay_pdf_with_options(&src, &document, &out, PdfOverlayOptions { visible: false })
            .unwrap();
        let pdf = lopdf::Document::load(&out).unwrap();
        let page_id = *pdf.get_pages().values().next().unwrap();
        let content = pdf.get_page_content(page_id);
        assert!(content.windows(4).any(|window| window == b"3 Tr"));
        assert!(content
            .windows(b"Selectable OCR".len())
            .any(|window| window == b"Selectable OCR"));
        std::fs::remove_file(src).ok();
        std::fs::remove_file(out).ok();
    }
}
