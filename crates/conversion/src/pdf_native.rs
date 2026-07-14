use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use std::collections::BTreeMap;
use std::path::Path;

/// Extract text content from a PDF file, producing a Document AST.
pub fn extract_pdf_text(path: impl AsRef<Path>) -> Result<Document> {
    let pdf = lopdf::Document::load(path.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to load PDF: {}", e)))?;
    extract_pdf_document(pdf)
}

/// Extract native PDF text from an in-memory buffer.
pub fn extract_pdf_text_bytes(bytes: &[u8]) -> Result<Document> {
    let pdf = lopdf::Document::load_mem(bytes)
        .map_err(|e| SnipperError::InvalidFormat(format!("Failed to load PDF: {e}")))?;
    extract_pdf_document(pdf)
}

fn extract_pdf_document(pdf: lopdf::Document) -> Result<Document> {
    let mut pages_out = Vec::new();

    for (&page_num, &object_id) in pdf.get_pages().iter() {
        let page_idx = (page_num as usize).saturating_sub(1);
        let page_size = get_page_size(&pdf, object_id).unwrap_or((612.0, 792.0));

        let text_fragments = extract_page_text(&pdf, object_id, page_size.1)?;

        let blocks: Vec<Block> = coalesce_lines(text_fragments)
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
            layout: None,
            background_asset_id: None,
        });
    }

    let mut diagnostics = Vec::new();
    let text_blocks = pages_out
        .iter()
        .map(|page| page.blocks.len())
        .sum::<usize>();
    if text_blocks == 0 {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticLevel::Warning,
                W_UNSUPPORTED_FEATURE,
                "PDF contains no decodable native text; OCR fallback was not run",
            )
            .with_formats(Some("PDF"), Some("AST"))
            .with_recoverable(true)
            .with_remediation("Enable OCR fallback with configured PDF rendering and OCR models"),
        );
    } else {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticLevel::Info,
                W_LAYOUT_LOSS,
                "PDF reading order and line grouping were reconstructed heuristically",
            )
            .with_formats(Some("PDF"), Some("AST"))
            .with_recoverable(true),
        );
    }

    Ok(Document {
        metadata: Metadata::default(),
        pages: pages_out,
        assets: Vec::new(),
        diagnostics,
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
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
                let x1 = number(&arr[0]).unwrap_or(0.0);
                let y1 = number(&arr[1]).unwrap_or(0.0);
                let x2 = number(&arr[2]).unwrap_or(612.0);
                let y2 = number(&arr[3]).unwrap_or(792.0);
                return Some(((x2 - x1).abs(), (y2 - y1).abs()));
            }
        }
    }
    None
}

fn extract_page_text(
    pdf: &lopdf::Document,
    object_id: lopdf::ObjectId,
    page_height: f32,
) -> Result<Vec<TextFragment>> {
    let mut fragments = Vec::new();
    let fonts = pdf.get_page_fonts(object_id).map_err(|error| {
        SnipperError::Conversion(format!("Failed to resolve PDF fonts: {error}"))
    })?;
    let encodings: BTreeMap<Vec<u8>, lopdf::Encoding<'_>> = fonts
        .into_iter()
        .filter_map(|(name, font)| {
            font.get_font_encoding(pdf)
                .ok()
                .map(|encoding| (name, encoding))
        })
        .collect();

    let mut ctm = identity_matrix();
    let mut graphics_stack = Vec::new();
    let mut text_matrix = identity_matrix();
    let mut line_matrix = identity_matrix();
    let mut font_size = 12.0f32;
    let mut leading = 0.0f32;
    let mut text_rise = 0.0f32;
    let mut font_name: Option<Vec<u8>> = None;

    let content = pdf.get_page_content(object_id);

    let operations = lopdf::content::Content::decode(&content)
        .map_err(|e| SnipperError::Export(format!("Failed to decode PDF content: {}", e)))?;

    for op in &operations.operations {
        let operands = &op.operands;
        match op.operator.as_ref() {
            "q" => graphics_stack.push(ctm),
            "Q" => ctm = graphics_stack.pop().unwrap_or_else(identity_matrix),
            "BT" => {
                text_matrix = identity_matrix();
                line_matrix = identity_matrix();
            }
            "Td" | "TD" => {
                if operands.len() >= 2 {
                    let tx = number(&operands[0]).unwrap_or(0.0);
                    let ty = number(&operands[1]).unwrap_or(0.0);
                    if op.operator == "TD" {
                        leading = -ty;
                    }
                    line_matrix = multiply(line_matrix, translation(tx, ty));
                    text_matrix = line_matrix;
                }
            }
            "cm" => {
                if let Some(matrix) = matrix_from_operands(operands) {
                    ctm = multiply(ctm, matrix);
                }
            }
            "Tf" => {
                if operands.len() >= 2 {
                    font_name = operands[0].as_name().ok().map(ToOwned::to_owned);
                    font_size = number(&operands[1]).unwrap_or(12.0).abs();
                }
            }
            "Tm" => {
                if let Some(matrix) = matrix_from_operands(operands) {
                    text_matrix = matrix;
                    line_matrix = matrix;
                }
            }
            "TL" => leading = operands.first().and_then(number).unwrap_or(leading),
            "Ts" => text_rise = operands.first().and_then(number).unwrap_or(0.0),
            "T*" => {
                let line_advance = if leading == 0.0 {
                    font_size * 1.2
                } else {
                    leading
                };
                line_matrix = multiply(line_matrix, translation(0.0, -line_advance));
                text_matrix = line_matrix;
            }
            "Tj" => {
                show_text(
                    operands.first(),
                    &encodings,
                    font_name.as_deref(),
                    ctm,
                    &mut text_matrix,
                    text_rise,
                    font_size,
                    page_height,
                    &mut fragments,
                );
            }
            "'" => {
                let line_advance = if leading == 0.0 {
                    font_size * 1.2
                } else {
                    leading
                };
                line_matrix = multiply(line_matrix, translation(0.0, -line_advance));
                text_matrix = line_matrix;
                show_text(
                    operands.first(),
                    &encodings,
                    font_name.as_deref(),
                    ctm,
                    &mut text_matrix,
                    text_rise,
                    font_size,
                    page_height,
                    &mut fragments,
                );
            }
            "\"" => {
                let line_advance = if leading == 0.0 {
                    font_size * 1.2
                } else {
                    leading
                };
                line_matrix = multiply(line_matrix, translation(0.0, -line_advance));
                text_matrix = line_matrix;
                show_text(
                    operands.get(2),
                    &encodings,
                    font_name.as_deref(),
                    ctm,
                    &mut text_matrix,
                    text_rise,
                    font_size,
                    page_height,
                    &mut fragments,
                );
            }
            "TJ" => {
                let text = extract_tj_text(op, &encodings, font_name.as_deref());
                if !text.is_empty() {
                    let tw = estimate_text_width(&text, font_size);
                    let (x, pdf_y) = transform_point(multiply(ctm, text_matrix), 0.0, text_rise);
                    fragments.push(TextFragment {
                        text,
                        x,
                        y: page_height - pdf_y - font_size,
                        width: tw,
                        height: font_size * 1.2,
                        font_size,
                    });
                    text_matrix = multiply(text_matrix, translation(tw, 0.0));
                }
            }
            _ => {}
        }
    }

    Ok(fragments)
}

fn extract_text_obj(
    obj: &lopdf::Object,
    encodings: &BTreeMap<Vec<u8>, lopdf::Encoding<'_>>,
    font_name: Option<&[u8]>,
) -> Option<String> {
    match obj {
        lopdf::Object::String(bytes, _) => font_name
            .and_then(|name| encodings.get(name))
            .and_then(|encoding| lopdf::Document::decode_text(encoding, bytes).ok())
            .or_else(|| String::from_utf8(bytes.clone()).ok()),
        _ => None,
    }
}

fn extract_tj_text(
    op: &lopdf::content::Operation,
    encodings: &BTreeMap<Vec<u8>, lopdf::Encoding<'_>>,
    font_name: Option<&[u8]>,
) -> String {
    let mut text = String::new();
    if let Some(arr) = op.operands.first().and_then(|o| o.as_array().ok()) {
        for item in arr.iter() {
            if let Some(s) = extract_text_obj(item, encodings, font_name) {
                text.push_str(&s);
            } else if number(item).is_some_and(|adjustment| adjustment < -100.0) {
                text.push(' ');
            }
        }
    }
    text
}

#[allow(clippy::too_many_arguments)]
fn show_text(
    object: Option<&lopdf::Object>,
    encodings: &BTreeMap<Vec<u8>, lopdf::Encoding<'_>>,
    font_name: Option<&[u8]>,
    ctm: [f32; 6],
    text_matrix: &mut [f32; 6],
    text_rise: f32,
    font_size: f32,
    page_height: f32,
    fragments: &mut Vec<TextFragment>,
) {
    let Some(text) = object.and_then(|object| extract_text_obj(object, encodings, font_name))
    else {
        return;
    };
    let width = estimate_text_width(&text, font_size);
    let (x, pdf_y) = transform_point(multiply(ctm, *text_matrix), 0.0, text_rise);
    fragments.push(TextFragment {
        text,
        x,
        y: page_height - pdf_y - font_size,
        width,
        height: font_size * 1.2,
        font_size,
    });
    *text_matrix = multiply(*text_matrix, translation(width, 0.0));
}

fn number(object: &lopdf::Object) -> Option<f32> {
    object.as_float().ok()
}

fn identity_matrix() -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

fn translation(x: f32, y: f32) -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn matrix_from_operands(operands: &[lopdf::Object]) -> Option<[f32; 6]> {
    (operands.len() >= 6).then(|| {
        [
            number(&operands[0])?,
            number(&operands[1])?,
            number(&operands[2])?,
            number(&operands[3])?,
            number(&operands[4])?,
            number(&operands[5])?,
        ]
        .into()
    })?
}

fn multiply(left: [f32; 6], right: [f32; 6]) -> [f32; 6] {
    [
        left[0] * right[0] + left[2] * right[1],
        left[1] * right[0] + left[3] * right[1],
        left[0] * right[2] + left[2] * right[3],
        left[1] * right[2] + left[3] * right[3],
        left[0] * right[4] + left[2] * right[5] + left[4],
        left[1] * right[4] + left[3] * right[5] + left[5],
    ]
}

fn transform_point(matrix: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    )
}

fn coalesce_lines(mut fragments: Vec<TextFragment>) -> Vec<TextFragment> {
    fragments.sort_by(|left, right| {
        left.y
            .partial_cmp(&right.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.x
                    .partial_cmp(&right.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let mut lines: Vec<TextFragment> = Vec::new();
    for fragment in fragments {
        let tolerance = fragment.font_size.max(4.0) * 0.5;
        if let Some(line) = lines
            .iter_mut()
            .rev()
            .find(|line| (line.y - fragment.y).abs() <= tolerance)
        {
            let gap = fragment.x - (line.x + line.width);
            if gap > fragment.font_size * 0.25 && !line.text.ends_with(char::is_whitespace) {
                line.text.push(' ');
            }
            line.text.push_str(&fragment.text);
            line.width = (fragment.x + fragment.width - line.x).max(line.width);
            line.height = line.height.max(fragment.height);
        } else {
            lines.push(fragment);
        }
    }
    lines
}

fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::DocumentBuilder;
    use latexsnipper_export::{ExportService, VisualFormat};
    use lopdf::dictionary;

    #[test]
    fn native_pdf_extraction_decodes_text_and_float_coordinates() {
        let source = DocumentBuilder::new()
            .page(595.0, 842.0, |page| {
                page.text_paragraph("Native PDF text");
            })
            .build();
        let artifact = ExportService::export(&source, VisualFormat::Pdf).unwrap();
        let imported = extract_pdf_text_bytes(artifact.as_bytes().unwrap()).unwrap();
        let paragraph = imported
            .all_blocks()
            .into_iter()
            .find_map(|block| match block {
                Block::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .expect("native text paragraph");
        let text = paragraph
            .inlines
            .iter()
            .filter_map(|inline| match inline {
                Inline::Text(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(text.contains("Native PDF text"));
        let geometry = paragraph.geometry.expect("native text geometry");
        assert!(geometry.x > 0.0 && geometry.y > 0.0);
        assert!(imported
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == W_LAYOUT_LOSS));
    }

    #[test]
    fn scanned_pdf_emits_explicit_ocr_remediation() {
        let mut pdf = lopdf::Document::with_version("1.5");
        let pages_id = pdf.new_object_id();
        let page_id = pdf.add_object(lopdf::dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        });
        pdf.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(lopdf::dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id =
            pdf.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        pdf.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();

        let imported = extract_pdf_text_bytes(&bytes).unwrap();
        assert!(imported
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == W_UNSUPPORTED_FEATURE));
    }
}
