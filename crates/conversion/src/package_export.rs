use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Write};

use base64::Engine as _;
use latexsnipper_ast::{
    AssetFormat, AssetId, AssetStorage, Block, Document, ExportArtifact, ExportFormat,
    FidelityClaim, FidelityDimensions, FidelityLevel, FidelityMeasurement, FormatCapability,
    GeneratedContent, Inline, InputFormat, LossKind, MediaAsset,
};
use latexsnipper_export::{ExportService, VisualFormat};
use latexsnipper_foundation::{Result, SnipperError};
use sha2::{Digest, Sha256};
use zip::write::FileOptions;

use crate::{DocumentConverter, OutputFormat};

/// Unified AST export registry for semantic, visual, and OOXML package formats.
pub struct DocumentExportService;

impl DocumentExportService {
    pub fn export(document: &Document, format: ExportFormat) -> Result<ExportArtifact> {
        match format {
            ExportFormat::Svg => ExportService::export(document, VisualFormat::Svg),
            ExportFormat::Pdf => ExportService::export(document, VisualFormat::Pdf),
            ExportFormat::Png => ExportService::export(document, VisualFormat::Png),
            ExportFormat::PlainText => ExportService::export(document, VisualFormat::PlainText),
            ExportFormat::Docx => binary_artifact(
                "docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                write_docx(document)?,
                crate::converter::collect_converter_diagnostics(document),
            ),
            ExportFormat::Pptx => binary_artifact(
                "pptx",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                write_pptx(document)?,
                crate::converter::collect_converter_diagnostics(document),
            ),
            ExportFormat::Xlsx => binary_artifact(
                "xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                write_xlsx(document)?,
                crate::converter::collect_converter_diagnostics(document),
            ),
            ExportFormat::AstJson => text_artifact(
                "json",
                "application/json",
                serde_json::to_string_pretty(document)
                    .map_err(|error| SnipperError::Export(error.to_string()))?,
            ),
            ExportFormat::Markdown => semantic_artifact(document, OutputFormat::MarkdownBlock),
            ExportFormat::Latex => semantic_artifact(document, OutputFormat::Latex),
            ExportFormat::Typst => semantic_artifact(document, OutputFormat::Typst),
            ExportFormat::Html => semantic_artifact(document, OutputFormat::Html),
            ExportFormat::MathML => semantic_artifact(document, OutputFormat::MathML),
            ExportFormat::OMML => semantic_artifact(document, OutputFormat::OMML),
            other => Err(SnipperError::UnsupportedFormat(format!(
                "export format {other:?} is not registered"
            ))),
        }
    }

    pub fn supported_formats() -> &'static [ExportFormat] {
        crate::REGISTERED_EXPORT_FORMATS
    }

    /// Resolve a user-facing label through the executable export registry.
    pub fn format_from_label(label: &str) -> Option<ExportFormat> {
        crate::CapabilityRegistry::resolve_export(label)
    }

    /// Return the canonical CLI/API label for a registered export format.
    pub const fn format_label(format: ExportFormat) -> &'static str {
        crate::export_format_label(format)
    }

    /// Return whether a registered format emits opaque binary bytes.
    pub const fn is_binary(format: ExportFormat) -> bool {
        crate::export_format_is_binary(format)
    }

    /// Generate capabilities directly from the callable importer/exporter registries.
    pub fn capability_matrix() -> latexsnipper_ast::CapabilityMatrix {
        let mut entries = Vec::new();
        for &input in crate::DocumentImporter::supported_formats() {
            for &output in Self::supported_formats() {
                entries.push(capability(input, output));
            }
        }
        latexsnipper_ast::CapabilityMatrix {
            schema_version: "3.0.0".to_string(),
            entries,
        }
    }
}

fn capability(input: InputFormat, output: ExportFormat) -> FormatCapability {
    let visual = matches!(
        output,
        ExportFormat::Svg | ExportFormat::Pdf | ExportFormat::Png
    );
    let office_output = matches!(
        output,
        ExportFormat::Docx | ExportFormat::Pptx | ExportFormat::Xlsx
    );
    let office_input = matches!(
        input,
        InputFormat::OfficeDocx | InputFormat::OfficePptx | InputFormat::OfficeXlsx
    );
    let raster_input = matches!(
        input,
        InputFormat::ImagePng
            | InputFormat::ImageJpeg
            | InputFormat::ImageWebp
            | InputFormat::ImageBmp
            | InputFormat::ImageTiff
            | InputFormat::ImageGif
    );
    let available = !raster_input || output == ExportFormat::AstJson;
    let mut known_loss = Vec::new();
    if matches!(
        output,
        ExportFormat::PlainText
            | ExportFormat::Markdown
            | ExportFormat::Latex
            | ExportFormat::Typst
    ) {
        known_loss.push(LossKind::LayoutLoss);
    }
    if office_input || office_output {
        known_loss.push(LossKind::OfficeObjectPreviewOnly);
    }
    if output == ExportFormat::PlainText {
        known_loss.push(LossKind::StyleLoss);
    }
    let mut notes = vec![if office_input || office_output {
        "Opaque OOXML parts are preserved when preservation mode is enabled".to_string()
    } else {
        "Registered in-process importer/exporter path".to_string()
    }];
    let mut required_features = Vec::new();
    let mut external_dependencies = Vec::new();
    if raster_input && !available {
        notes.push(
            "Direct raster import preserves the source asset but does not run OCR; use the recognize workflow first"
                .to_string(),
        );
        required_features.push("ocr-recognition".to_string());
        external_dependencies.push("configured OCR models".to_string());
    }
    FormatCapability {
        input: Some(input_label(input).to_string()),
        output: Some(output_label(output).to_string()),
        available,
        supports_formula: available && output != ExportFormat::PlainText,
        supports_table: available && !matches!(output, ExportFormat::MathML | ExportFormat::OMML),
        supports_image: available
            && !matches!(
                output,
                ExportFormat::PlainText | ExportFormat::MathML | ExportFormat::OMML
            ),
        supports_svg: available
            && matches!(
                output,
                ExportFormat::Svg | ExportFormat::Png | ExportFormat::Html
            ),
        supports_style: available
            && !matches!(
                output,
                ExportFormat::PlainText | ExportFormat::MathML | ExportFormat::OMML
            ),
        supports_layout: available && (visual || office_output),
        supports_office_objects: false,
        fidelity: if visual || raster_input {
            FidelityLevel::VisualOnly
        } else if office_input || office_output {
            FidelityLevel::BestEffort
        } else {
            FidelityLevel::SemanticOnly
        },
        fidelity_dimensions: fidelity_dimensions(input, output, available),
        known_loss,
        notes,
        required_features,
        external_dependencies,
        platform_restrictions: Vec::new(),
        experimental: visual || office_output,
    }
}

fn fidelity_dimensions(
    input: InputFormat,
    output: ExportFormat,
    available: bool,
) -> FidelityDimensions {
    if !available {
        let unsupported = measurement(
            FidelityClaim::Unsupported,
            None,
            &[],
            &["format pair is not callable without an explicit recognition stage"],
        );
        return FidelityDimensions {
            structural_validity: unsupported.clone(),
            semantic_preservation: unsupported.clone(),
            layout_preservation: unsupported.clone(),
            visual_fidelity: unsupported.clone(),
            editability: unsupported.clone(),
            round_trip_fidelity: unsupported,
        };
    }

    let office_input = matches!(
        input,
        InputFormat::OfficeDocx | InputFormat::OfficePptx | InputFormat::OfficeXlsx
    );
    let pdf_input = input == InputFormat::Pdf;
    let office_output = matches!(
        output,
        ExportFormat::Docx | ExportFormat::Pptx | ExportFormat::Xlsx
    );
    let pdf_output = output == ExportFormat::Pdf;

    if !(office_input || pdf_input || office_output || pdf_output) {
        return FidelityDimensions::default();
    }

    let structural_validity = if office_output {
        measurement(
            FidelityClaim::Verified,
            None,
            &["gate:office-package-reopen"],
            &["package validity is not visual parity"],
        )
    } else if pdf_output {
        measurement(
            FidelityClaim::Verified,
            None,
            &["gate:pdf-reopen"],
            &["PDF syntax validity does not prove rendering parity"],
        )
    } else {
        measurement(
            FidelityClaim::Partial,
            None,
            &["gate:source-import"],
            &["the target format has no pair-specific reopen gate"],
        )
    };

    let semantic_preservation = measurement(
        FidelityClaim::Partial,
        None,
        &["gate:semantic-ast-comparison"],
        &["unsupported source constructs require diagnostics or opaque preservation"],
    );
    let layout_preservation = measurement(
        FidelityClaim::Partial,
        None,
        &["gate:layout-comparison"],
        &["pagination, floating objects, and application layout engines may differ"],
    );
    let visual_fidelity = measurement(
        FidelityClaim::NotMeasured,
        None,
        &["gate:optional-visual-render"],
        &["visual comparison requires an external renderer and is not inferred from reopen"],
    );
    let editability = if pdf_output {
        measurement(
            FidelityClaim::Unsupported,
            Some(0.0),
            &[],
            &["PDF export is a visual document path, not an editable Office object model"],
        )
    } else if office_output {
        measurement(
            FidelityClaim::Partial,
            None,
            &["gate:editable-ast-node-coverage"],
            &["charts, SmartArt, OLE, and other opaque parts are not semantically editable"],
        )
    } else {
        FidelityMeasurement::default()
    };
    let round_trip_fidelity = if pdf_output || pdf_input {
        measurement(
            FidelityClaim::Unsupported,
            None,
            &[],
            &["native PDF extraction and reflow export are not lossless round-trip operations"],
        )
    } else if office_input || office_output {
        measurement(
            FidelityClaim::Partial,
            None,
            &["gate:office-round-trip"],
            &["generated core parts take precedence over preserved opaque parts"],
        )
    } else {
        FidelityMeasurement::default()
    };

    FidelityDimensions {
        structural_validity,
        semantic_preservation,
        layout_preservation,
        visual_fidelity,
        editability,
        round_trip_fidelity,
    }
}

fn measurement(
    claim: FidelityClaim,
    score: Option<f64>,
    evidence: &[&str],
    limitations: &[&str],
) -> FidelityMeasurement {
    FidelityMeasurement {
        claim,
        score,
        evidence: evidence.iter().map(|value| (*value).to_string()).collect(),
        limitations: limitations
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}

fn input_label(format: InputFormat) -> &'static str {
    match format {
        InputFormat::ImagePng => "PNG",
        InputFormat::ImageJpeg => "JPEG",
        InputFormat::ImageWebp => "WebP",
        InputFormat::ImageBmp => "BMP",
        InputFormat::ImageTiff => "TIFF",
        InputFormat::ImageGif => "GIF",
        InputFormat::ImageSvg => "SVG",
        InputFormat::Pdf => "PDF",
        InputFormat::OfficeDocx => "DOCX",
        InputFormat::OfficePptx => "PPTX",
        InputFormat::OfficeXlsx => "XLSX",
        InputFormat::Html => "HTML",
        InputFormat::Markdown => "Markdown",
        InputFormat::Latex => "LaTeX",
        InputFormat::Typst => "Typst",
        InputFormat::MathML => "MathML",
        InputFormat::OMML => "OMML",
        InputFormat::JsonAst => "JSON AST",
        InputFormat::PlainText => "Plain text",
        InputFormat::RawPixels | InputFormat::Clipboard | InputFormat::Unknown => "Unregistered",
    }
}

fn output_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::AstJson => "JSON AST",
        ExportFormat::PlainText => "Plain text",
        ExportFormat::Markdown => "Markdown",
        ExportFormat::Latex => "LaTeX",
        ExportFormat::Typst => "Typst",
        ExportFormat::Html => "HTML",
        ExportFormat::MathML => "MathML",
        ExportFormat::OMML => "OMML",
        ExportFormat::Svg => "SVG",
        ExportFormat::Pdf => "PDF",
        ExportFormat::Png => "PNG",
        ExportFormat::Docx => "DOCX",
        ExportFormat::Pptx => "PPTX",
        ExportFormat::Xlsx => "XLSX",
        _ => "Unregistered",
    }
}

fn semantic_artifact(document: &Document, format: OutputFormat) -> Result<ExportArtifact> {
    DocumentConverter::new(format)
        .convert_artifact(document)
        .map_err(SnipperError::Export)
}

fn binary_artifact(
    format: &str,
    mime: &str,
    bytes: Vec<u8>,
    diagnostics: Vec<latexsnipper_ast::Diagnostic>,
) -> Result<ExportArtifact> {
    let checksum = format!("{:x}", Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    Ok(ExportArtifact {
        format: format.to_string(),
        primary_path: None,
        content: Some(GeneratedContent::Binary(bytes)),
        text: None,
        assets: Vec::new(),
        diagnostics,
        mime_type: Some(mime.to_string()),
        checksum_sha256: Some(checksum),
        size_bytes: Some(size),
    })
}

fn text_artifact(format: &str, mime: &str, text: String) -> Result<ExportArtifact> {
    let checksum = format!("{:x}", Sha256::digest(text.as_bytes()));
    let size = text.len() as u64;
    Ok(ExportArtifact {
        format: format.to_string(),
        primary_path: None,
        content: Some(GeneratedContent::Text(text.clone())),
        text: Some(text),
        assets: Vec::new(),
        diagnostics: Vec::new(),
        mime_type: Some(mime.to_string()),
        checksum_sha256: Some(checksum),
        size_bytes: Some(size),
    })
}

struct PackageWriter {
    zip: zip::ZipWriter<Cursor<Vec<u8>>>,
    written: HashSet<String>,
}

impl PackageWriter {
    fn new() -> Self {
        Self {
            zip: zip::ZipWriter::new(Cursor::new(Vec::new())),
            written: HashSet::new(),
        }
    }

    fn part(&mut self, name: &str, bytes: impl AsRef<[u8]>) -> Result<()> {
        validate_part_name(name)?;
        if !self.written.insert(name.to_string()) {
            return Ok(());
        }
        self.zip
            .start_file(name, FileOptions::default())
            .map_err(|error| SnipperError::Export(error.to_string()))?;
        self.zip
            .write_all(bytes.as_ref())
            .map_err(|error| SnipperError::Export(error.to_string()))
    }

    fn preserve_opaque(&mut self, document: &Document) -> Result<()> {
        for asset in &document.assets {
            if asset.format != AssetFormat::OoxmlPart {
                continue;
            }
            let Some(name) = asset
                .metadata
                .get("package_part")
                .and_then(|value| value.as_str())
            else {
                continue;
            };
            if self.written.contains(name) {
                continue;
            }
            if let Some(bytes) = asset_bytes(asset)? {
                self.part(name, bytes)?;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u8>> {
        self.zip
            .finish()
            .map(|cursor| cursor.into_inner())
            .map_err(|error| SnipperError::Export(error.to_string()))
    }
}

fn validate_part_name(name: &str) -> Result<()> {
    let path = std::path::Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(SnipperError::Export(format!(
            "unsafe OOXML part path '{name}'"
        )));
    }
    Ok(())
}

fn write_docx(document: &Document) -> Result<Vec<u8>> {
    let mut package = PackageWriter::new();
    let media = collect_media(document, "word/media", "rIdImage");
    package.part("[Content_Types].xml", docx_content_types(&media))?;
    package.part(
        "_rels/.rels",
        root_rels("word/document.xml", "officeDocument"),
    )?;
    package.part("docProps/core.xml", core_properties())?;
    package.part("docProps/app.xml", app_properties("Microsoft Office Word"))?;
    package.part("word/document.xml", docx_document_xml(document, &media)?)?;
    package.part("word/_rels/document.xml.rels", docx_rels(&media))?;
    package.part("word/styles.xml", DOCX_STYLES)?;
    package.part("word/numbering.xml", DOCX_NUMBERING)?;
    package.part("word/settings.xml", DOCX_SETTINGS)?;
    package.part("word/fontTable.xml", DOCX_FONT_TABLE)?;
    package.part("word/theme/theme1.xml", OFFICE_THEME)?;
    write_media(&mut package, &media)?;
    package.preserve_opaque(document)?;
    package.finish()
}

fn docx_document_xml(document: &Document, media: &[MediaPart]) -> Result<String> {
    let media_map: HashMap<&AssetId, &MediaPart> =
        media.iter().map(|part| (&part.id, part)).collect();
    let mut body = String::new();
    for (page_index, page) in document.pages.iter().enumerate() {
        if page_index > 0 {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
        for block in &page.blocks {
            body.push_str(&word_block(block, &media_map)?);
        }
    }
    let section = document.pages.first().and_then(|page| page.layout.as_ref()).map_or_else(
        || "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"/></w:sectPr>".to_string(),
        |layout| format!(
            "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\"/></w:sectPr>",
            points_to_twips(layout.width), points_to_twips(layout.height), points_to_twips(layout.margin.top), points_to_twips(layout.margin.right), points_to_twips(layout.margin.bottom), points_to_twips(layout.margin.left)
        ),
    );
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><w:body>{body}{section}</w:body></w:document>"
    ))
}

fn word_block(block: &Block, media: &HashMap<&AssetId, &MediaPart>) -> Result<String> {
    match block {
        Block::Heading(heading) => Ok(format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>{}</w:p>",
            heading.level.clamp(1, 6),
            word_inlines(&heading.inlines, media)?
        )),
        Block::Paragraph(paragraph) => Ok(format!(
            "<w:p>{}</w:p>",
            word_inlines(&paragraph.inlines, media)?
        )),
        Block::Formula(formula) => {
            let omml = formula_omml(&formula.formula)?;
            if omml.contains("<m:oMathPara") {
                Ok(omml)
            } else {
                Ok(format!("<m:oMathPara>{omml}</m:oMathPara>"))
            }
        }
        Block::Table(table) => Ok(crate::write_word_table_ooxml(table)),
        Block::List(list) => {
            let mut xml = String::new();
            for item in &list.items {
                let text = item
                    .content
                    .iter()
                    .map(block_plain_text)
                    .collect::<Vec<_>>()
                    .join(" ");
                xml.push_str(&format!("<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{}\"/></w:numPr></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", if list.is_ordered() { 1 } else { 2 }, xml_escape(&text)));
            }
            Ok(xml)
        }
        Block::Figure(figure) => Ok(figure
            .asset_id
            .as_ref()
            .and_then(|id| media.get(id))
            .map(|part| format!("<w:p>{}</w:p>", word_drawing(part, None, None)))
            .unwrap_or_default()),
        Block::PageBreak(_) => Ok("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>".to_string()),
        _ => Ok(format!(
            "<w:p><w:r><w:t>{}</w:t></w:r></w:p>",
            xml_escape(&block_plain_text(block))
        )),
    }
}

fn word_inlines(inlines: &[Inline], media: &HashMap<&AssetId, &MediaPart>) -> Result<String> {
    let mut output = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => {
                let mut properties = String::new();
                if text.bold == Some(true) {
                    properties.push_str("<w:b/>");
                }
                if text.italic == Some(true) {
                    properties.push_str("<w:i/>");
                }
                if text.underline == Some(true) {
                    properties.push_str("<w:u w:val=\"single\"/>");
                }
                if text.strikethrough == Some(true) {
                    properties.push_str("<w:strike/>");
                }
                if let Some(style) = &text.style {
                    if let Some(font) = &style.font_family {
                        properties.push_str(&format!(
                            "<w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\"/>",
                            xml_escape(font),
                            xml_escape(font)
                        ));
                    }
                    if let Some(size) = style.font_size {
                        properties.push_str(&format!(
                            "<w:sz w:val=\"{}\"/>",
                            (size * 2.0).round() as u32
                        ));
                    }
                    if let Some(color) = &style.color {
                        properties.push_str(&format!(
                            "<w:color w:val=\"{}\"/>",
                            color.value.trim_start_matches('#')
                        ));
                    }
                }
                output.push_str(&format!(
                    "<w:r><w:rPr>{properties}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    xml_escape(&text.text)
                ));
            }
            Inline::Formula(formula) => output.push_str(&formula_omml(formula)?),
            Inline::Image(image) => {
                if let Some(part) = image.asset_id.as_ref().and_then(|id| media.get(id)) {
                    output.push_str(&word_drawing(part, image.width, image.height));
                }
            }
            Inline::LineBreak | Inline::SoftBreak => output.push_str("<w:r><w:br/></w:r>"),
            Inline::Span(span) => output.push_str(&word_inlines(&span.content, media)?),
            Inline::Link(link) => output.push_str(&word_inlines(&link.content, media)?),
            Inline::Superscript(content) | Inline::Subscript(content) => {
                output.push_str(&word_inlines(content, media)?)
            }
            _ => {}
        }
    }
    Ok(output)
}

fn formula_omml(formula: &latexsnipper_ast::Formula) -> Result<String> {
    if let Some(raw) = formula
        .source_info
        .as_ref()
        .filter(|source| source.raw_source_format.as_deref() == Some("omml"))
        .and_then(|source| source.raw_source.as_deref())
    {
        return Ok(raw.to_string());
    }
    DocumentConverter::convert_latex_string(formula.as_latex(), OutputFormat::OMML)
}

fn word_drawing(part: &MediaPart, width: Option<f32>, height: Option<f32>) -> String {
    let cx = ((width.unwrap_or(240.0) * 9_525.0).round() as u64).max(1);
    let cy = ((height.unwrap_or(160.0) * 9_525.0).round() as u64).max(1);
    format!("<w:r><w:drawing><wp:inline><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"1\" name=\"{}\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>", xml_escape(&part.filename), xml_escape(&part.filename), part.rid)
}

fn write_pptx(document: &Document) -> Result<Vec<u8>> {
    let mut package = PackageWriter::new();
    let media = collect_media(document, "ppt/media", "rIdImage");
    package.part(
        "[Content_Types].xml",
        pptx_content_types(document.pages.len(), &media),
    )?;
    package.part(
        "_rels/.rels",
        root_rels("ppt/presentation.xml", "officeDocument"),
    )?;
    package.part("docProps/core.xml", core_properties())?;
    package.part(
        "docProps/app.xml",
        app_properties("Microsoft Office PowerPoint"),
    )?;
    package.part(
        "ppt/presentation.xml",
        presentation_xml(document.pages.len()),
    )?;
    package.part(
        "ppt/_rels/presentation.xml.rels",
        presentation_rels(document.pages.len()),
    )?;
    package.part("ppt/slideMasters/slideMaster1.xml", PPTX_MASTER)?;
    package.part(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        PPTX_MASTER_RELS,
    )?;
    package.part("ppt/slideLayouts/slideLayout1.xml", PPTX_LAYOUT)?;
    package.part(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        PPTX_LAYOUT_RELS,
    )?;
    package.part("ppt/theme/theme1.xml", OFFICE_THEME)?;
    for (index, page) in document.pages.iter().enumerate() {
        package.part(
            &format!("ppt/slides/slide{}.xml", index + 1),
            slide_xml(page, &media),
        )?;
        package.part(
            &format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
            slide_rels(&media),
        )?;
    }
    write_media(&mut package, &media)?;
    package.preserve_opaque(document)?;
    package.finish()
}

fn slide_xml(page: &latexsnipper_ast::Page, media: &[MediaPart]) -> String {
    let mut shapes = String::new();
    let mut shape_id = 2u32;
    let mut y = 300_000i64;
    for block in &page.blocks {
        if let Block::Figure(figure) = block {
            if let Some(part) = figure
                .asset_id
                .as_ref()
                .and_then(|id| media.iter().find(|part| &part.id == id))
            {
                shapes.push_str(&ppt_picture(shape_id, part, y));
                shape_id += 1;
                y += 2_000_000;
                continue;
            }
        }
        let text = block_plain_text(block);
        if !text.is_empty() {
            shapes.push_str(&ppt_text_box(shape_id, &text, y));
            shape_id += 1;
            y += 700_000;
        }
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>")
}

fn ppt_text_box(id: u32, text: &str, y: i64) -> String {
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"400000\" y=\"{y}\"/><a:ext cx=\"8000000\" cy=\"600000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>", xml_escape(text))
}

fn ppt_picture(id: u32, part: &MediaPart, y: i64) -> String {
    format!("<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"400000\" y=\"{y}\"/><a:ext cx=\"3000000\" cy=\"1800000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>", xml_escape(&part.filename), part.rid)
}

fn write_xlsx(document: &Document) -> Result<Vec<u8>> {
    let mut package = PackageWriter::new();
    let sheet_count = document.pages.len().max(1);
    package.part("[Content_Types].xml", xlsx_content_types(sheet_count))?;
    package.part(
        "_rels/.rels",
        root_rels("xl/workbook.xml", "officeDocument"),
    )?;
    package.part("docProps/core.xml", core_properties())?;
    package.part("docProps/app.xml", app_properties("Microsoft Excel"))?;
    package.part("xl/workbook.xml", workbook_xml(document))?;
    package.part("xl/_rels/workbook.xml.rels", workbook_rels(sheet_count))?;
    package.part("xl/styles.xml", XLSX_STYLES)?;
    if document.pages.is_empty() {
        package.part("xl/worksheets/sheet1.xml", empty_sheet())?;
    } else {
        for (index, page) in document.pages.iter().enumerate() {
            package.part(
                &format!("xl/worksheets/sheet{}.xml", index + 1),
                worksheet_xml(page),
            )?;
        }
    }
    package.preserve_opaque(document)?;
    package.finish()
}

fn worksheet_xml(page: &latexsnipper_ast::Page) -> String {
    let table = page.blocks.iter().find_map(|block| match block {
        Block::Table(table) => Some(table),
        _ => None,
    });
    let mut rows_xml = String::new();
    let mut merges = Vec::new();
    if let Some(table) = table {
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut cells_xml = String::new();
            let mut column = 0u32;
            for cell in &row.cells {
                let reference = cell_ref(column, row_index as u32);
                let value = cell
                    .content
                    .iter()
                    .map(block_plain_text)
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(formula) = &cell.formula {
                    cells_xml.push_str(&format!(
                        "<c r=\"{reference}\"><f>{}</f><v>{}</v></c>",
                        xml_escape(formula),
                        xml_escape(&value)
                    ));
                } else if matches!(cell.data_type, Some(latexsnipper_ast::CellDataType::Number)) {
                    cells_xml.push_str(&format!(
                        "<c r=\"{reference}\"><v>{}</v></c>",
                        xml_escape(&value)
                    ));
                } else if matches!(
                    cell.data_type,
                    Some(latexsnipper_ast::CellDataType::Boolean)
                ) {
                    cells_xml.push_str(&format!(
                        "<c r=\"{reference}\" t=\"b\"><v>{}</v></c>",
                        if value.eq_ignore_ascii_case("true") {
                            "1"
                        } else {
                            "0"
                        }
                    ));
                } else if matches!(cell.data_type, Some(latexsnipper_ast::CellDataType::Error)) {
                    cells_xml.push_str(&format!(
                        "<c r=\"{reference}\" t=\"e\"><v>{}</v></c>",
                        xml_escape(&value)
                    ));
                } else {
                    cells_xml.push_str(&format!("<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", xml_escape(&value)));
                }
                if cell.colspan > 1 || cell.rowspan > 1 {
                    merges.push(format!(
                        "{}:{}",
                        reference,
                        cell_ref(
                            column + cell.colspan - 1,
                            row_index as u32 + cell.rowspan - 1
                        )
                    ));
                }
                column += cell.colspan.max(1);
            }
            rows_xml.push_str(&format!("<row r=\"{}\">{cells_xml}</row>", row_index + 1));
        }
    }
    let merge_xml = if merges.is_empty() {
        String::new()
    } else {
        format!(
            "<mergeCells count=\"{}\">{}</mergeCells>",
            merges.len(),
            merges
                .iter()
                .map(|range| format!("<mergeCell ref=\"{range}\"/>"))
                .collect::<String>()
        )
    };
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{rows_xml}</sheetData>{merge_xml}</worksheet>")
}

fn empty_sheet() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/></worksheet>"
}

fn block_plain_text(block: &Block) -> String {
    match block {
        Block::Heading(value) => inline_plain_text(&value.inlines),
        Block::Paragraph(value) => inline_plain_text(&value.inlines),
        Block::Formula(value) => value.formula.as_latex().to_string(),
        Block::Code(value) => value.code.clone(),
        Block::Quote(value) => value
            .blocks
            .iter()
            .map(block_plain_text)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Handwriting(value) => inline_plain_text(&value.inlines),
        _ => String::new(),
    }
}

fn inline_plain_text(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(value) => value.text.clone(),
            Inline::Formula(value) => value.as_latex().to_string(),
            Inline::Span(value) => inline_plain_text(&value.content),
            Inline::Link(value) => inline_plain_text(&value.content),
            Inline::LineBreak | Inline::SoftBreak => "\n".to_string(),
            _ => String::new(),
        })
        .collect()
}

#[derive(Clone)]
struct MediaPart {
    id: AssetId,
    rid: String,
    path: String,
    filename: String,
    extension: String,
    mime: String,
    bytes: Vec<u8>,
}

fn collect_media(document: &Document, directory: &str, rid_prefix: &str) -> Vec<MediaPart> {
    document
        .assets
        .iter()
        .filter(|asset| asset.format != AssetFormat::OoxmlPart)
        .filter_map(|asset| {
            let bytes = asset_bytes(asset).ok().flatten()?;
            let (extension, mime) = asset_extension(asset);
            let index = document
                .assets
                .iter()
                .position(|candidate| candidate.id == asset.id)?
                + 1;
            let filename = format!("image{index}.{extension}");
            Some(MediaPart {
                id: asset.id.clone(),
                rid: format!("{rid_prefix}{index}"),
                path: format!("{directory}/{filename}"),
                filename,
                extension: extension.to_string(),
                mime: mime.to_string(),
                bytes,
            })
        })
        .collect()
}

fn asset_bytes(asset: &MediaAsset) -> Result<Option<Vec<u8>>> {
    match &asset.storage {
        AssetStorage::InlineBase64 { data } => base64::engine::general_purpose::STANDARD
            .decode(data)
            .map(Some)
            .map_err(|error| {
                SnipperError::Export(format!("invalid base64 asset {}: {error}", asset.id.0))
            }),
        AssetStorage::FilePath { path } => std::fs::read(path).map(Some).map_err(|error| {
            SnipperError::Export(format!("failed to read asset '{path}': {error}"))
        }),
        _ => Ok(None),
    }
}

fn asset_extension(asset: &MediaAsset) -> (&'static str, &'static str) {
    match asset.format {
        AssetFormat::Png => ("png", "image/png"),
        AssetFormat::Jpeg => ("jpg", "image/jpeg"),
        AssetFormat::Gif => ("gif", "image/gif"),
        AssetFormat::Bmp => ("bmp", "image/bmp"),
        AssetFormat::Tiff => ("tiff", "image/tiff"),
        AssetFormat::Svg => ("svg", "image/svg+xml"),
        _ => ("bin", "application/octet-stream"),
    }
}

fn write_media(package: &mut PackageWriter, media: &[MediaPart]) -> Result<()> {
    for part in media {
        package.part(&part.path, &part.bytes)?;
    }
    Ok(())
}

fn root_rels(target: &str, kind: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>")
}

fn core_properties() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>LaTeXSnipper Export</dc:title><dc:creator>LaTeXSnipper</dc:creator></cp:coreProperties>"
}
fn app_properties(app: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>{}</Application></Properties>", xml_escape(app))
}

fn docx_content_types(media: &[MediaPart]) -> String {
    content_types(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        "/word/document.xml",
        media,
        &[
            (
                "/word/styles.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
            ),
            (
                "/word/numbering.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
            ),
            (
                "/word/settings.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
            ),
            (
                "/word/fontTable.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml",
            ),
            (
                "/word/theme/theme1.xml",
                "application/vnd.openxmlformats-officedocument.theme+xml",
            ),
            (
                "/word/embeddings/oleObject1.bin",
                "application/vnd.openxmlformats-officedocument.oleObject",
            ),
        ],
    )
}
fn pptx_content_types(slides: usize, media: &[MediaPart]) -> String {
    let mut extra = vec![
        (
            "/ppt/slideMasters/slideMaster1.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
        ),
        (
            "/ppt/slideLayouts/slideLayout1.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
        ),
        (
            "/ppt/theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml",
        ),
        (
            "/ppt/notesSlides/notesSlide1.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml",
        ),
        (
            "/ppt/charts/chart1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        ),
        (
            "/ppt/diagrams/data1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml",
        ),
        (
            "/ppt/embeddings/oleObject1.bin",
            "application/vnd.openxmlformats-officedocument.oleObject",
        ),
    ];
    let names: Vec<String> = (1..=slides)
        .map(|index| format!("/ppt/slides/slide{index}.xml"))
        .collect();
    for name in &names {
        extra.push((
            name,
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        ));
    }
    content_types(
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        "/ppt/presentation.xml",
        media,
        &extra,
    )
}
fn content_types(
    main: &str,
    main_part: &str,
    media: &[MediaPart],
    extra: &[(&str, &str)],
) -> String {
    let mut defaults = HashSet::new();

    let mut xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ",
        "ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>"
    )
    .to_string();

    for part in media {
        if defaults.insert(part.extension.clone()) {
            xml.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                part.extension, part.mime
            ));
        }
    }

    // Declare the OOXML main document part Content-Type.
    xml.push_str(&format!(
        "<Override PartName=\"{main_part}\" ContentType=\"{main}\"/>"
    ));

    for (name, mime) in extra {
        xml.push_str(&format!(
            "<Override PartName=\"{name}\" ContentType=\"{mime}\"/>"
        ));
    }

    xml.push_str(concat!(
        "<Override PartName=\"/docProps/core.xml\" ",
        "ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
        "<Override PartName=\"/docProps/app.xml\" ",
        "ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>",
        "</Types>"
    ));

    xml
}

fn docx_rels(media: &[MediaPart]) -> String {
    let mut xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships ",
        "xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rIdStyles\" ",
        "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" ",
        "Target=\"styles.xml\"/>",
        "<Relationship Id=\"rIdNumbering\" ",
        "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" ",
        "Target=\"numbering.xml\"/>",
        "<Relationship Id=\"rIdSettings\" ",
        "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" ",
        "Target=\"settings.xml\"/>",
        "<Relationship Id=\"rIdFontTable\" ",
        "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable\" ",
        "Target=\"fontTable.xml\"/>",
        "<Relationship Id=\"rIdTheme\" ",
        "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" ",
        "Target=\"theme/theme1.xml\"/>"
    )
    .to_string();

    for part in media {
        xml.push_str(&format!(
            concat!(
                "<Relationship Id=\"{}\" ",
                "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" ",
                "Target=\"media/{}\"/>"
            ),
            part.rid, part.filename
        ));
    }

    xml.push_str("</Relationships>");

    xml
}
fn presentation_xml(slides: usize) -> String {
    let ids = (1..=slides)
        .map(|index| format!("<p:sldId id=\"{}\" r:id=\"rIdSlide{index}\"/>", 255 + index))
        .collect::<String>();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rIdMaster\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>")
}
fn presentation_rels(slides: usize) -> String {
    let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdMaster\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>".to_string();
    for index in 1..=slides {
        xml.push_str(&format!("<Relationship Id=\"rIdSlide{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{index}.xml\"/>"));
    }
    xml.push_str("</Relationships>");
    xml
}
fn slide_rels(media: &[MediaPart]) -> String {
    let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdLayout\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>".to_string();
    for part in media {
        xml.push_str(&format!("<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/{}\"/>", part.rid, part.filename));
    }
    xml.push_str("</Relationships>");
    xml
}
fn xlsx_content_types(sheets: usize) -> String {
    let mut overrides = vec![
        (
            "/xl/workbook.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        ),
        (
            "/xl/styles.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        ),
        (
            "/xl/tables/table1.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
        ),
        (
            "/xl/drawings/drawing1.xml",
            "application/vnd.openxmlformats-officedocument.drawing+xml",
        ),
        (
            "/xl/charts/chart1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        ),
        (
            "/xl/pivotTables/pivotTable1.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml",
        ),
        (
            "/xl/vbaProject.bin",
            "application/vnd.ms-office.vbaProject",
        ),
        (
            "/xl/embeddings/oleObject1.bin",
            "application/vnd.openxmlformats-officedocument.oleObject",
        ),
    ];
    let sheet_ct =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
    for index in 1..=sheets {
        let name = format!("/xl/worksheets/sheet{index}.xml");
        let leaked: &'static str = Box::leak(name.into_boxed_str());
        overrides.push((leaked, sheet_ct));
    }
    content_types(
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        "/xl/workbook.xml",
        &[],
        &overrides,
    )
}
fn workbook_xml(document: &Document) -> String {
    let count = document.pages.len().max(1);
    let sheets = (1..=count)
        .map(|index| {
            format!("<sheet name=\"Sheet {index}\" sheetId=\"{index}\" r:id=\"rId{index}\"/>")
        })
        .collect::<String>();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>{sheets}</sheets><calcPr calcId=\"191029\"/></workbook>")
}
fn workbook_rels(sheets: usize) -> String {
    let mut xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">".to_string();
    for index in 1..=sheets {
        xml.push_str(&format!("<Relationship Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>"));
    }
    xml.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>", sheets + 1));
    xml
}
fn cell_ref(mut column: u32, row: u32) -> String {
    let mut letters = String::new();
    loop {
        letters.insert(0, (b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    format!("{letters}{}", row + 1)
}
fn points_to_twips(value: f32) -> u32 {
    (value.max(0.0) * 20.0).round() as u32
}
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const DOCX_STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style></w:styles>";
const DOCX_NUMBERING: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"•\"/></w:lvl></w:abstractNum><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>";
const DOCX_SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:compat/></w:settings>";
const DOCX_FONT_TABLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:fonts xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:font w:name=\"Calibri\"/></w:fonts>";
const OFFICE_THEME: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Office\"><a:themeElements><a:clrScheme name=\"Office\"><a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1><a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1></a:clrScheme><a:fontScheme name=\"Office\"><a:majorFont/><a:minorFont/></a:fontScheme><a:fmtScheme name=\"Office\"><a:fillStyleLst/><a:lnStyleLst/><a:effectStyleLst/><a:bgFillStyleLst/></a:fmtScheme></a:themeElements></a:theme>";
const PPTX_MASTER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:sldLayoutIdLst><p:sldLayoutId id=\"1\" r:id=\"rIdLayout\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>";
const PPTX_MASTER_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdLayout\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rIdTheme\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"../theme/theme1.xml\"/></Relationships>";
const PPTX_LAYOUT: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldLayout xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" type=\"blank\"><p:cSld name=\"Blank\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld></p:sldLayout>";
const PPTX_LAYOUT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdMaster\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>";
const XLSX_STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border/></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs></styleSheet>";

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::{DocumentBuilder, MediaRole, TableBlock, TableCell, TableRow};
    use std::io::Read as _;

    fn sample_document() -> Document {
        DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.heading(1, "Title");
                page.text_paragraph("Hello Office");
                page.display_formula(r"\frac{a}{b}");
            })
            .build()
    }

    #[test]
    fn generated_docx_reopens_with_formula_and_text() {
        let artifact =
            DocumentExportService::export(&sample_document(), ExportFormat::Docx).unwrap();
        let document = crate::read_docx_bytes(artifact.as_bytes().unwrap()).unwrap();
        assert!(document.block_count() >= 2);
        assert!(document
            .all_blocks()
            .iter()
            .any(|block| matches!(block, Block::Formula(_))));
        assert_package_entries(
            artifact.as_bytes().unwrap(),
            &[
                "[Content_Types].xml",
                "_rels/.rels",
                "word/document.xml",
                "word/styles.xml",
                "word/numbering.xml",
            ],
        );
        assert_office_package_contract(
            artifact.as_bytes().unwrap(),
            "word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        );
    }

    #[test]
    fn generated_pptx_reopens_with_slide_text() {
        let artifact =
            DocumentExportService::export(&sample_document(), ExportFormat::Pptx).unwrap();
        let document = crate::read_pptx_bytes(artifact.as_bytes().unwrap()).unwrap();
        assert_eq!(document.pages.len(), 1);
        assert!(document.block_count() > 0);
        assert_package_entries(
            artifact.as_bytes().unwrap(),
            &[
                "[Content_Types].xml",
                "_rels/.rels",
                "ppt/presentation.xml",
                "ppt/slides/slide1.xml",
                "ppt/slideMasters/slideMaster1.xml",
            ],
        );
        assert_office_package_contract(
            artifact.as_bytes().unwrap(),
            "ppt/presentation.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        );
    }

    #[test]
    fn generated_xlsx_reopens_with_table_semantics() {
        let mut document = Document::new();
        let mut page = latexsnipper_ast::Page::new(800.0, 600.0, 1);
        page.blocks.push(Block::Table(TableBlock {
            rows: vec![TableRow {
                cells: vec![TableCell {
                    content: vec![Block::Paragraph(latexsnipper_ast::ParagraphBlock {
                        inlines: vec![Inline::Text(latexsnipper_ast::TextRun::new("Value"))],
                        geometry: None,
                        source: None,
                        style: None,
                    })],
                    colspan: 1,
                    rowspan: 1,
                    data_type: Some(latexsnipper_ast::CellDataType::Text),
                    formula: None,
                    style: None,
                    border_style: None,
                    border_width: None,
                    border_color: None,
                    background: None,
                    alignment: None,
                    geometry: None,
                    source: None,
                }],
                height: None,
                is_header: false,
            }],
            columns: Vec::new(),
            caption: None,
            style: None,
            geometry: None,
            source: None,
        }));
        document.add_page(page);
        let artifact = DocumentExportService::export(&document, ExportFormat::Xlsx).unwrap();
        let reopened = crate::read_xlsx_bytes(artifact.as_bytes().unwrap()).unwrap();
        assert_eq!(reopened.pages.len(), 1);
        assert!(reopened
            .all_blocks()
            .iter()
            .any(|block| matches!(block, Block::Table(_))));
        assert_package_entries(
            artifact.as_bytes().unwrap(),
            &[
                "[Content_Types].xml",
                "_rels/.rels",
                "xl/workbook.xml",
                "xl/styles.xml",
                "xl/worksheets/sheet1.xml",
            ],
        );
        assert_office_package_contract(
            artifact.as_bytes().unwrap(),
            "xl/workbook.xml",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        );
    }

    #[test]
    fn opaque_package_parts_survive_without_overwriting_generated_core_parts() {
        let mut document = sample_document();
        document.assets.push(MediaAsset {
            id: AssetId("opaque-custom".to_string()),
            format: AssetFormat::OoxmlPart,
            mime_type: Some("application/octet-stream".to_string()),
            role: MediaRole::Unknown,
            storage: AssetStorage::InlineBase64 {
                data: base64::engine::general_purpose::STANDARD.encode(b"opaque-data"),
            },
            width: None,
            height: None,
            dpi: None,
            color_space: None,
            checksum: None,
            alt_text: None,
            metadata: HashMap::from([(
                "package_part".to_string(),
                serde_json::Value::String("customXml/item1.bin".to_string()),
            )]),
        });
        let artifact = DocumentExportService::export(&document, ExportFormat::Docx).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(artifact.as_bytes().unwrap())).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut archive.by_name("customXml/item1.bin").unwrap(),
            &mut bytes,
        )
        .unwrap();
        assert_eq!(bytes, b"opaque-data");
        assert!(archive.by_name("word/document.xml").is_ok());
    }

    #[test]
    fn capability_matrix_is_generated_from_callable_registries() {
        let matrix = DocumentExportService::capability_matrix();
        assert_eq!(
            matrix.entries.len(),
            crate::DocumentImporter::supported_formats().len()
                * DocumentExportService::supported_formats().len()
        );
        assert!(matrix.query("DOCX", "PNG").is_some());
        assert!(matrix.query("DOCX", "PNG").unwrap().available);
        assert!(matrix.query("PNG", "JSON AST").unwrap().available);
        let png_markdown = matrix.query("PNG", "Markdown").unwrap();
        assert!(!png_markdown.available);
        assert!(png_markdown
            .required_features
            .contains(&"ocr-recognition".to_string()));

        let document = sample_document();
        for &format in DocumentExportService::supported_formats() {
            let artifact = DocumentExportService::export(&document, format)
                .unwrap_or_else(|error| panic!("registered {format:?} failed: {error}"));
            assert!(artifact.as_bytes().is_some_and(|bytes| !bytes.is_empty()));
        }
    }

    #[test]
    fn office_and_pdf_pairs_report_six_independent_fidelity_dimensions() {
        let matrix = DocumentExportService::capability_matrix();
        assert_eq!(matrix.schema_version, "3.0.0");
        let office = matrix.query("DOCX", "DOCX").unwrap();
        assert_eq!(
            office.fidelity_dimensions.structural_validity.claim,
            FidelityClaim::Verified
        );
        assert_eq!(
            office.fidelity_dimensions.visual_fidelity.claim,
            FidelityClaim::NotMeasured
        );
        assert_eq!(
            office.fidelity_dimensions.editability.claim,
            FidelityClaim::Partial
        );

        let pdf = matrix.query("PDF", "PDF").unwrap();
        assert_eq!(
            pdf.fidelity_dimensions.structural_validity.claim,
            FidelityClaim::Verified
        );
        assert_eq!(
            pdf.fidelity_dimensions.visual_fidelity.claim,
            FidelityClaim::NotMeasured
        );
        assert_eq!(
            pdf.fidelity_dimensions.editability.claim,
            FidelityClaim::Unsupported
        );

        let json = serde_json::to_value(office).unwrap();
        let dimensions = json.get("fidelity_dimensions").unwrap();
        for key in [
            "structuralValidity",
            "semanticPreservation",
            "layoutPreservation",
            "visualFidelity",
            "editability",
            "roundTripFidelity",
        ] {
            assert!(dimensions.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn readme_registry_markers_match_executable_formats() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap();
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
        let inputs = crate::DocumentImporter::supported_formats()
            .iter()
            .map(|format| input_label(*format))
            .collect::<Vec<_>>()
            .join(",");
        let outputs = DocumentExportService::supported_formats()
            .iter()
            .map(|format| output_label(*format))
            .collect::<Vec<_>>()
            .join(",");

        assert!(
            readme.contains(&format!("<!-- capability-inputs: {inputs} -->")),
            "README input registry marker drifted"
        );
        assert!(
            readme.contains(&format!("<!-- capability-outputs: {outputs} -->")),
            "README output registry marker drifted"
        );
    }

    fn assert_package_entries(bytes: &[u8], required: &[&str]) {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        for entry in required {
            assert!(
                archive.by_name(entry).is_ok(),
                "missing package part {entry}"
            );
        }
    }

    fn package_entry_text(bytes: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut entry = archive.by_name(name).unwrap();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        text
    }

    fn assert_office_package_contract(bytes: &[u8], main_part: &str, main_content_type: &str) {
        assert_package_entries(bytes, &["[Content_Types].xml", "_rels/.rels", main_part]);

        let root_rels = package_entry_text(bytes, "_rels/.rels");

        assert!(root_rels.contains(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
        ));

        assert!(root_rels.contains(&format!("Target=\"{main_part}\"")));

        let content_types = package_entry_text(bytes, "[Content_Types].xml");

        assert!(
            content_types.contains(&format!("PartName=\"/{main_part}\"")),
            "missing main-part content type for {main_part}",
        );

        assert!(content_types.contains(&format!("ContentType=\"{main_content_type}\"")),);
    }
}
