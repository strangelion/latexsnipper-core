use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
    // lopdf returns BTreeMap<page_number, ObjectId> where
    // page_number is a u32 key and ObjectId is a (u32, u16) tuple value.
    let pages_map = doc.get_pages();
    let mut pages = Vec::with_capacity(pages_map.len());

    for (page_number, object_id) in &pages_map {
        // object_id = &(id: u32, generation: u16) — the PDF object reference
        if let Ok(page_obj) = doc.dereference(&lopdf::Object::Reference(*object_id)) {
            if let Ok(media_box) = extract_media_box(page_obj.1) {
                pages.push(PdfPageInfo {
                    page_number: *page_number,
                    width: media_box[2] - media_box[0],
                    height: media_box[3] - media_box[1],
                });
            }
        }
    }

    // pages_map is BTreeMap, already sorted by page_number key.
    Ok(pages)
}

/// RAII guard to ensure temporary files are cleaned up on drop.
struct TempFileGuard {
    path: Option<PathBuf>,
    owns_temp: bool,
}

impl TempFileGuard {
    fn file(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            owns_temp: false,
        }
    }
    fn temp(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            owns_temp: true,
        }
    }
    fn path(&self) -> &Path {
        self.path.as_ref().expect("TempFileGuard empty")
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.owns_temp {
            if let Some(ref p) = self.path {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// Decode all pages from a PDF source into images.
///
/// Uses external tools (pdftoppm or mutool) for rendering.
/// Requires one of these tools to be installed:
/// - `pdftoppm` from poppler-utils
/// - `mutool` from MuPDF
pub fn decode_pdf(source: PdfSource, dpi: u32) -> Result<Vec<SnipperImage>> {
    let guard = prepare_pdf_path(source)?;
    let pdf_path = guard.path().to_path_buf();

    let page_info = get_pdf_page_info(PdfSource::File(&pdf_path))?;
    let mut images = Vec::with_capacity(page_info.len());

    for info in &page_info {
        match crate::pdf_render::render_pdf_page(&pdf_path, info.page_number, dpi) {
            Ok(img) => images.push(img),
            Err(e) => {
                log::warn!("Failed to render page {}: {}", info.page_number, e);
                return Err(e);
            }
        }
    }

    // Temp file is cleaned up automatically when `guard` drops
    Ok(images)
}

/// Decode a single page from a PDF source into an image.
pub fn decode_pdf_page(source: PdfSource, page: u32, dpi: u32) -> Result<SnipperImage> {
    let guard = prepare_pdf_path(source)?;
    let pdf_path = guard.path().to_path_buf();

    let result = crate::pdf_render::render_pdf_page(&pdf_path, page, dpi);

    // Temp file cleaned up on guard drop
    result
}

/// Prepare a PDF path from the source.
fn prepare_pdf_path(source: PdfSource) -> Result<TempFileGuard> {
    match source {
        PdfSource::File(path) => Ok(TempFileGuard::file(path.to_path_buf())),
        PdfSource::Memory(bytes) => {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let tmp_path = std::env::temp_dir().join(format!("latexsnipper-pdf-{}.pdf", stamp));
            std::fs::write(&tmp_path, bytes)
                .map_err(|e| SnipperError::Image(format!("Failed to write temp PDF: {}", e)))?;
            Ok(TempFileGuard::temp(tmp_path))
        }
    }
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
    use lopdf::{Dictionary, Object};

    #[test]
    fn test_extract_media_box_from_dict() {
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
        let mut doc = lopdf::Document::new();
        let path = std::env::temp_dir().join("test_empty.pdf");
        doc.save(&path).unwrap();
        let info = get_pdf_page_info(PdfSource::File(&path));
        std::fs::remove_file(&path).unwrap();
        assert!(info.is_ok());
        assert!(info.unwrap().is_empty());
    }
}
