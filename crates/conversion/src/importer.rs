use std::io::{Cursor, Read};
use std::path::Path;

use base64::Engine as _;
use latexsnipper_ast::{
    AssetFormat, AssetId, AssetStorage, Block, DiagnosticLevel, Document, Formula, FormulaBlock,
    ImportOptions, Inline, InputFormat, MediaAsset, MediaRole, Metadata, NodeIdGenerator, Page,
    ParagraphBlock, TextRun,
};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_syntax::latex::LatexParser;
use latexsnipper_syntax::Parser as _;

use crate::{
    extract_pdf_text_bytes, parse_html_to_document, parse_markdown_to_document,
    parse_mathml_to_latex, parse_omml_to_latex, parse_svg_to_shapes, parse_typst_to_latex,
    read_docx_bytes, read_pptx_bytes, read_xlsx_bytes,
};

/// Unified, signature-first document importer for file paths and memory buffers.
pub struct DocumentImporter;

impl DocumentImporter {
    pub fn supported_formats() -> &'static [InputFormat] {
        &[
            InputFormat::ImagePng,
            InputFormat::ImageJpeg,
            InputFormat::ImageWebp,
            InputFormat::ImageBmp,
            InputFormat::ImageTiff,
            InputFormat::ImageGif,
            InputFormat::ImageSvg,
            InputFormat::Pdf,
            InputFormat::OfficeDocx,
            InputFormat::OfficePptx,
            InputFormat::OfficeXlsx,
            InputFormat::Html,
            InputFormat::Markdown,
            InputFormat::Latex,
            InputFormat::Typst,
            InputFormat::MathML,
            InputFormat::OMML,
            InputFormat::JsonAst,
            InputFormat::PlainText,
        ]
    }

    pub fn from_path(path: impl AsRef<Path>, options: ImportOptions) -> Result<Document> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| SnipperError::Io(format!("Failed to read '{}': {e}", path.display())))?;
        let format = Self::detect_format(&bytes, Some(path))?;
        Self::from_bytes(&bytes, Some(format), options)
    }

    pub fn from_bytes(
        bytes: &[u8],
        format_hint: Option<InputFormat>,
        options: ImportOptions,
    ) -> Result<Document> {
        let detected = Self::detect_format(bytes, None)?;
        let format = resolve_hint(detected, format_hint)?;
        enforce_input_limits(bytes, format, &options)?;

        let mut document = match format {
            InputFormat::OfficeDocx => read_docx_bytes(bytes)?,
            InputFormat::OfficePptx => read_pptx_bytes(bytes)?,
            InputFormat::OfficeXlsx => read_xlsx_bytes(bytes)?,
            InputFormat::Pdf => {
                let pdf = lopdf::Document::load_mem(bytes).map_err(|e| {
                    SnipperError::InvalidFormat(format!("Invalid PDF structure: {e}"))
                })?;
                if pdf.is_encrypted() {
                    return Err(SnipperError::EncryptedFile("PDF".to_string()));
                }
                extract_pdf_text_bytes(bytes)?
            }
            InputFormat::Markdown => parse_text(bytes, parse_markdown_to_document)?,
            InputFormat::Html => parse_text(bytes, parse_html_to_document)?,
            InputFormat::Latex => {
                let text = utf8_text(bytes)?;
                LatexParser.parse(text)?
            }
            InputFormat::Typst => document_with_formula(parse_typst_to_latex(utf8_text(bytes)?)),
            InputFormat::MathML => document_with_formula(
                parse_mathml_to_latex(utf8_text(bytes)?)
                    .map_err(|e| SnipperError::InvalidFormat(format!("Invalid MathML: {e}")))?,
            ),
            InputFormat::OMML => document_with_formula(
                parse_omml_to_latex(utf8_text(bytes)?)
                    .map_err(|e| SnipperError::InvalidFormat(format!("Invalid OMML: {e}")))?,
            ),
            InputFormat::ImageSvg => {
                let svg = utf8_text(bytes)?;
                let blocks = parse_svg_to_shapes(svg)
                    .into_iter()
                    .map(Block::Shape)
                    .collect();
                document_with_blocks(blocks)
            }
            InputFormat::JsonAst => serde_json::from_slice(bytes).map_err(|e| {
                SnipperError::InvalidFormat(format!("Invalid Document JSON AST: {e}"))
            })?,
            InputFormat::PlainText => document_with_text(utf8_text(bytes)?),
            InputFormat::ImagePng
            | InputFormat::ImageJpeg
            | InputFormat::ImageWebp
            | InputFormat::ImageBmp
            | InputFormat::ImageTiff
            | InputFormat::ImageGif => image_document(bytes, format),
            InputFormat::RawPixels | InputFormat::Clipboard | InputFormat::Unknown => {
                return Err(SnipperError::UnsupportedFormat(format!("{format:?}")));
            }
        };

        if options.preserve_unknown_parts && is_office(format) {
            preserve_package_parts(bytes, &mut document, options.max_decompressed_size)?;
        }
        apply_options(&mut document, &options)?;
        Ok(document)
    }

    pub fn detect_format(bytes: &[u8], path_hint: Option<&Path>) -> Result<InputFormat> {
        Self::detect_format_with_mime(bytes, path_hint, None)
    }

    pub fn detect_format_with_mime(
        bytes: &[u8],
        path_hint: Option<&Path>,
        mime_hint: Option<&str>,
    ) -> Result<InputFormat> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Ok(InputFormat::ImagePng);
        }
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Ok(InputFormat::ImageJpeg);
        }
        if bytes.starts_with(b"BM") {
            return Ok(InputFormat::ImageBmp);
        }
        if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
            return Ok(InputFormat::ImageTiff);
        }
        if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Ok(InputFormat::ImageWebp);
        }
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            return Ok(InputFormat::ImageGif);
        }
        if bytes.starts_with(b"%PDF-") {
            return Ok(InputFormat::Pdf);
        }
        if bytes.starts_with(b"PK\x03\x04") {
            return detect_ooxml_package(bytes);
        }

        if let Some(format) = mime_hint.and_then(format_from_mime) {
            return Ok(format);
        }

        if let Ok(text) = std::str::from_utf8(bytes) {
            let trimmed = text.trim_start_matches('\u{feff}').trim_start();
            if trimmed.starts_with("<svg")
                || trimmed.starts_with("<?xml") && trimmed.contains("<svg")
            {
                return Ok(InputFormat::ImageSvg);
            }
            if trimmed.starts_with("<math") || trimmed.contains("<math ") {
                return Ok(InputFormat::MathML);
            }
            if trimmed.contains("<m:oMath") || trimmed.contains("<m:oMathPara") {
                return Ok(InputFormat::OMML);
            }
            if trimmed.starts_with("<!DOCTYPE html")
                || trimmed.starts_with("<html")
                || trimmed.contains("<body")
            {
                return Ok(InputFormat::Html);
            }
            if (trimmed.starts_with('{') || trimmed.starts_with('['))
                && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
            {
                return Ok(InputFormat::JsonAst);
            }
        }

        Ok(path_hint
            .and_then(|path| path.extension())
            .and_then(|ext| ext.to_str())
            .and_then(format_from_extension)
            .unwrap_or(InputFormat::Unknown))
    }
}

fn detect_ooxml_package(bytes: &[u8]) -> Result<InputFormat> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| SnipperError::InvalidFormat(format!("Invalid ZIP package: {e}")))?;
    let mut docx = false;
    let mut pptx = false;
    let mut xlsx = false;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|e| SnipperError::InvalidFormat(format!("Invalid ZIP entry: {e}")))?;
        let name = file.name();
        if name == "EncryptionInfo" || name == "EncryptedPackage" {
            return Err(SnipperError::EncryptedFile("OOXML package".to_string()));
        }
        docx |= name == "word/document.xml";
        pptx |= name == "ppt/presentation.xml";
        xlsx |= name == "xl/workbook.xml";
    }
    match (docx, pptx, xlsx) {
        (true, false, false) => Ok(InputFormat::OfficeDocx),
        (false, true, false) => Ok(InputFormat::OfficePptx),
        (false, false, true) => Ok(InputFormat::OfficeXlsx),
        _ => Err(SnipperError::InvalidFormat(
            "ZIP package is not an unambiguous DOCX, PPTX, or XLSX package".to_string(),
        )),
    }
}

fn resolve_hint(detected: InputFormat, hint: Option<InputFormat>) -> Result<InputFormat> {
    match (detected, hint) {
        (InputFormat::Unknown, Some(hint)) => Ok(hint),
        (InputFormat::Unknown, None) => Err(SnipperError::UnsupportedFormat(
            "unable to detect input format".to_string(),
        )),
        (detected, Some(hint)) if detected != hint => Err(SnipperError::InvalidFormat(format!(
            "format hint {hint:?} does not match detected {detected:?}"
        ))),
        (detected, _) => Ok(detected),
    }
}

fn enforce_input_limits(bytes: &[u8], format: InputFormat, options: &ImportOptions) -> Result<()> {
    if !is_office(format) && bytes.len() as u64 > options.max_text_size {
        return Err(SnipperError::LimitExceeded(format!(
            "input is {} bytes; limit is {}",
            bytes.len(),
            options.max_text_size
        )));
    }
    if is_office(format) {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| SnipperError::InvalidFormat(format!("Invalid OOXML ZIP: {e}")))?;
        let mut total = 0u64;
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(|e| {
                SnipperError::InvalidFormat(format!("Invalid OOXML ZIP entry: {e}"))
            })?;
            if file.enclosed_name().is_none() {
                return Err(SnipperError::InvalidFormat(format!(
                    "unsafe package path: {}",
                    file.name()
                )));
            }
            total = total
                .checked_add(file.size())
                .ok_or_else(|| SnipperError::LimitExceeded("package size overflow".to_string()))?;
            if total > options.max_decompressed_size {
                return Err(SnipperError::LimitExceeded(format!(
                    "package expands to more than {} bytes",
                    options.max_decompressed_size
                )));
            }
        }
    }
    Ok(())
}

fn preserve_package_parts(bytes: &[u8], document: &mut Document, limit: u64) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| SnipperError::InvalidFormat(format!("Invalid OOXML ZIP: {e}")))?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| SnipperError::InvalidFormat(format!("Invalid OOXML part: {e}")))?;
        if file.is_dir() {
            continue;
        }
        total += file.size();
        if total > limit {
            return Err(SnipperError::LimitExceeded(
                "opaque package part limit exceeded".to_string(),
            ));
        }
        let name = file.name().to_string();
        let mut part = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut part)
            .map_err(|e| SnipperError::Io(format!("Failed to preserve '{name}': {e}")))?;
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "package_part".to_string(),
            serde_json::Value::String(name.clone()),
        );
        document.assets.push(MediaAsset {
            id: AssetId(format!("opaque-part-{index}")),
            format: AssetFormat::OoxmlPart,
            mime_type: None,
            role: MediaRole::Unknown,
            storage: AssetStorage::InlineBase64 {
                data: base64::engine::general_purpose::STANDARD.encode(part),
            },
            width: None,
            height: None,
            dpi: None,
            color_space: None,
            checksum: None,
            alt_text: Some(name),
            metadata,
        });
    }
    Ok(())
}

fn apply_options(document: &mut Document, options: &ImportOptions) -> Result<()> {
    if let Some(range) = options.page_range {
        if range.start == 0 || range.end < range.start {
            return Err(SnipperError::InvalidFormat(format!(
                "invalid page range {}-{}",
                range.start, range.end
            )));
        }
        let indices: Vec<usize> = (range.start..=range.end)
            .map(|page| (page - 1) as usize)
            .collect();
        *document = document.filter_pages(&indices);
    }
    if !options.preserve_assets {
        document.assets.clear();
    }
    if !options.preserve_layout {
        for page in &mut document.pages {
            page.layout = None;
            for block in &mut page.blocks {
                clear_block_geometry(block);
            }
        }
    }
    if options.strict
        && document
            .diagnostics
            .iter()
            .any(|diag| diag.level != DiagnosticLevel::Info)
    {
        return Err(SnipperError::Conversion(
            "strict import rejected fidelity diagnostics".to_string(),
        ));
    }
    Ok(())
}

fn clear_block_geometry(block: &mut Block) {
    match block {
        Block::Paragraph(value) => value.geometry = None,
        Block::Heading(value) => value.geometry = None,
        Block::Formula(value) => value.geometry = None,
        Block::Table(value) => value.geometry = None,
        Block::Figure(value) => value.geometry = None,
        _ => {}
    }
}

fn parse_text(bytes: &[u8], parser: impl FnOnce(&str) -> Document) -> Result<Document> {
    Ok(parser(utf8_text(bytes)?))
}

fn utf8_text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes)
        .map_err(|e| SnipperError::InvalidFormat(format!("text input is not UTF-8: {e}")))
}

fn document_with_formula(latex: String) -> Document {
    document_with_blocks(vec![Block::Formula(FormulaBlock {
        formula: Formula::latex(latex),
        label: None,
        number: None,
        environment: None,
        geometry: None,
        source: None,
    })])
}

fn document_with_text(text: &str) -> Document {
    document_with_blocks(vec![Block::Paragraph(ParagraphBlock {
        inlines: vec![Inline::Text(TextRun::new(text))],
        geometry: None,
        source: None,
        style: None,
    })])
}

fn document_with_blocks(blocks: Vec<Block>) -> Document {
    Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: 0.0,
            height: 0.0,
            blocks,
            page_number: Some(1),
            layout: None,
            background_asset_id: None,
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
    }
}

fn image_document(bytes: &[u8], format: InputFormat) -> Document {
    let (asset_format, mime) = match format {
        InputFormat::ImagePng => (AssetFormat::Png, "image/png"),
        InputFormat::ImageJpeg => (AssetFormat::Jpeg, "image/jpeg"),
        InputFormat::ImageWebp => (AssetFormat::Webp, "image/webp"),
        InputFormat::ImageBmp => (AssetFormat::Bmp, "image/bmp"),
        InputFormat::ImageTiff => (AssetFormat::Tiff, "image/tiff"),
        InputFormat::ImageGif => (AssetFormat::Gif, "image/gif"),
        _ => unreachable!("caller restricts image formats"),
    };
    let id = AssetId("source-image".to_string());
    let mut document = document_with_blocks(Vec::new());
    document.pages[0].background_asset_id = Some(id.clone());
    document.assets.push(MediaAsset {
        id,
        format: asset_format,
        mime_type: Some(mime.to_string()),
        role: MediaRole::Scan,
        storage: AssetStorage::InlineBase64 {
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        },
        width: None,
        height: None,
        dpi: None,
        color_space: None,
        checksum: None,
        alt_text: Some("Imported source image".to_string()),
        metadata: Default::default(),
    });
    document
}

fn is_office(format: InputFormat) -> bool {
    matches!(
        format,
        InputFormat::OfficeDocx | InputFormat::OfficePptx | InputFormat::OfficeXlsx
    )
}

fn format_from_mime(mime: &str) -> Option<InputFormat> {
    match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(InputFormat::ImagePng),
        "image/jpeg" => Some(InputFormat::ImageJpeg),
        "image/webp" => Some(InputFormat::ImageWebp),
        "image/bmp" => Some(InputFormat::ImageBmp),
        "image/tiff" => Some(InputFormat::ImageTiff),
        "image/svg+xml" => Some(InputFormat::ImageSvg),
        "application/pdf" => Some(InputFormat::Pdf),
        "text/markdown" => Some(InputFormat::Markdown),
        "text/html" => Some(InputFormat::Html),
        "application/mathml+xml" => Some(InputFormat::MathML),
        "application/json" => Some(InputFormat::JsonAst),
        "text/plain" => Some(InputFormat::PlainText),
        _ => None,
    }
}

fn format_from_extension(extension: &str) -> Option<InputFormat> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some(InputFormat::ImagePng),
        "jpg" | "jpeg" => Some(InputFormat::ImageJpeg),
        "webp" => Some(InputFormat::ImageWebp),
        "bmp" => Some(InputFormat::ImageBmp),
        "tif" | "tiff" => Some(InputFormat::ImageTiff),
        "gif" => Some(InputFormat::ImageGif),
        "svg" => Some(InputFormat::ImageSvg),
        "pdf" => Some(InputFormat::Pdf),
        "docx" => Some(InputFormat::OfficeDocx),
        "pptx" => Some(InputFormat::OfficePptx),
        "xlsx" => Some(InputFormat::OfficeXlsx),
        "md" | "markdown" => Some(InputFormat::Markdown),
        "html" | "htm" => Some(InputFormat::Html),
        "tex" | "latex" => Some(InputFormat::Latex),
        "typ" | "typst" => Some(InputFormat::Typst),
        "mathml" | "mml" => Some(InputFormat::MathML),
        "omml" => Some(InputFormat::OMML),
        "json" => Some(InputFormat::JsonAst),
        "txt" => Some(InputFormat::PlainText),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_win_over_wrong_extensions() {
        assert_eq!(
            DocumentImporter::detect_format(b"\x89PNG\r\n\x1a\nrest", Some(Path::new("wrong.pdf")))
                .unwrap(),
            InputFormat::ImagePng
        );
    }

    #[test]
    fn mismatched_hint_is_typed_error() {
        let error = DocumentImporter::from_bytes(
            b"%PDF-invalid",
            Some(InputFormat::ImagePng),
            ImportOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error, SnipperError::InvalidFormat(_)));
    }

    #[test]
    fn imports_memory_markdown_and_page_range() {
        let options = ImportOptions {
            page_range: Some(latexsnipper_ast::PageRange { start: 1, end: 1 }),
            ..Default::default()
        };
        let document = DocumentImporter::from_bytes(
            b"# Heading\n\nText",
            Some(InputFormat::Markdown),
            options,
        )
        .unwrap();
        assert_eq!(document.pages.len(), 1);
        assert!(document.block_count() >= 2);
    }

    #[test]
    fn imports_json_ast_from_memory() {
        let source = document_with_text("AST");
        let bytes = serde_json::to_vec(&source).unwrap();
        let imported = DocumentImporter::from_bytes(
            &bytes,
            Some(InputFormat::JsonAst),
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(imported.block_count(), 1);
    }
}
