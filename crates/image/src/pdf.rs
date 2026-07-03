use std::path::Path;

use latexsnipper_foundation::{Result, SnipperError};

use crate::image::SnipperImage;

/// PDF input source.
pub enum PdfSource<'a> {
    File(&'a Path),
    Memory(&'a [u8]),
}

/// Information about a PDF page.
#[derive(Debug, Clone)]
pub struct PdfPageInfo {
    pub page_number: u32,
    pub width: f32,
    pub height: f32,
}

/// Get information about all pages in a PDF without rendering them.
///
/// Pages are returned in page number order (1, 2, 3, ...), which is the
/// canonical PDF page order and may differ from internal object IDs.
pub fn get_pdf_page_info(source: PdfSource) -> Result<Vec<PdfPageInfo>> {
    let doc = load_document(source)?;
    let page_ids: Vec<u32> = doc.get_pages().keys().cloned().collect();
    let mut pages = Vec::with_capacity(page_ids.len());

    for (idx, &page_id) in page_ids.iter().enumerate() {
        if let Ok(page_obj) = doc.dereference(&lopdf::Object::Reference((page_id, 0))) {
            if let Ok(media_box) = extract_media_box(page_obj.1) {
                pages.push(PdfPageInfo {
                    page_number: (idx + 1) as u32,
                    width: media_box[2] - media_box[0],
                    height: media_box[3] - media_box[1],
                });
            }
        }
    }

    // lopdf::get_pages returns BTreeMap<ObjectId, PageNumber> keyed by
    // internal object IDs, not page order. Sort by page_number to ensure
    // canonical PDF page ordering.
    pages.sort_by_key(|a| a.page_number);

    Ok(pages)
}

/// Decode all pages from a PDF source into images.
///
/// # Note
///
/// PDF rendering is **not yet implemented**. This function returns an error
/// because generating white placeholder images would silently break all
/// downstream recognition (OCR, formula detection, etc.).
///
/// To render PDF pages, one of the following renderers needs to be integrated:
/// - [pdfium-render](https://github.com/nicohman/pdfium-render) (Rust bindings to PDFium)
/// - [poppler-rs](https://crates.io/crates/poppler-rs) (Rust bindings to poppler)
/// - Shelling out to `pdftoppm` or `mutool draw` (Poppler/MuPDF CLI)
///
/// Until then, use `get_pdf_page_info` to read PDF metadata, and process
/// individual page images through `Snipper::from_file` or `Engine::recognize`.
pub fn decode_pdf(_source: PdfSource, _dpi: u32) -> Result<Vec<SnipperImage>> {
    Err(SnipperError::Image(
        "PDF page rendering is not yet implemented. \
         Use get_pdf_page_info() to read metadata, or convert PDF pages to images \
         externally (e.g. pdftoppm, pdfium) and process each page image individually. \
         See the doc comment on decode_pdf() for integration options."
            .into(),
    ))
}

/// Decode a single page from a PDF source into an image.
///
/// # Note
///
/// Same limitation as [`decode_pdf`] — returns an error because PDF content
/// rendering is not yet implemented.
pub fn decode_pdf_page(_source: PdfSource, _page: u32, _dpi: u32) -> Result<SnipperImage> {
    Err(SnipperError::Image(
        "PDF page rendering is not yet implemented. \
         See decode_pdf() for details."
            .into(),
    ))
}

fn load_document(source: PdfSource) -> Result<lopdf::Document> {
    match source {
        PdfSource::File(path) => lopdf::Document::load(path)
            .map_err(|e| SnipperError::Image(format!("Failed to load PDF: {}", e))),
        PdfSource::Memory(bytes) => lopdf::Document::load_mem(bytes)
            .map_err(|e| SnipperError::Image(format!("Failed to load PDF from memory: {}", e))),
    }
}

fn extract_media_box(page_obj: &lopdf::Object) -> Result<[f32; 4]> {
    match page_obj {
        lopdf::Object::Dictionary(dict) => {
            if let Ok(lopdf::Object::Array(arr)) = dict.get(b"MediaBox") {
                if arr.len() < 4 {
                    return Err(SnipperError::Image(
                        "MediaBox requires at least 4 values".into(),
                    ));
                }
                let mut values = [0.0f32; 4];
                for (i, val) in arr.iter().enumerate().take(4) {
                    if let lopdf::Object::Integer(n) = val {
                        values[i] = *n as f32;
                    } else if let lopdf::Object::Real(r) = val {
                        values[i] = *r;
                    }
                }
                // Validate: width and height must be positive
                if values[2] <= values[0] || values[3] <= values[1] {
                    return Err(SnipperError::Image(format!(
                        "Invalid MediaBox dimensions: [{}, {}, {}, {}]",
                        values[0], values[1], values[2], values[3]
                    )));
                }
                return Ok(values);
            }
            // Return error instead of hardcoded US Letter fallback
            Err(SnipperError::Image(
                "MediaBox not found in page dictionary, and inheritance from Pages tree \
                 is not yet implemented. Use get_pdf_page_info() for metadata extraction."
                    .into(),
            ))
        }
        _ => Err(SnipperError::Image("Page is not a dictionary".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_media_box_from_dict() {
        use lopdf::{Dictionary, Object};
        let mut dict = Dictionary::new();
        dict.set(
            b"MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(612),
                Object::Integer(792),
            ]),
        );
        let obj = Object::Dictionary(dict);
        let result = extract_media_box(&obj).unwrap();
        assert_eq!(result, [0.0, 0.0, 612.0, 792.0]);
    }

    #[test]
    fn test_extract_media_box_missing() {
        let obj = Object::Dictionary(Dictionary::new());
        assert!(extract_media_box(&obj).is_err());
    }

    #[test]
    fn test_extract_media_box_invalid_type() {
        let obj = Object::Null;
        assert!(extract_media_box(&obj).is_err());
    }

    #[test]
    fn test_get_pdf_page_info_empty() {
        // An empty document with no pages
        let doc = lopdf::Document::new();
        let path = std::env::temp_dir().join("test_empty.pdf");
        doc.save(&path).unwrap();
        let info = get_pdf_page_info(PdfSource::File(&path));
        std::fs::remove_file(&path).unwrap();
        assert!(info.is_ok());
        assert!(info.unwrap().is_empty());
    }
}
