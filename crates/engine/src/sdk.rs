//! LaTeXSnipper Core SDK — One-line Image to Export
//!
//! ```rust,no_run
//! use latexsnipper_engine::sdk::Snipper;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let result = Snipper::from_file("input.png")?
//!     .to_latex()?;
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

use latexsnipper_ast::*;
use latexsnipper_conversion::{
    DocumentConverter, DocumentExportService, DocumentImporter, OutputFormat,
};
use latexsnipper_export::{ExportService, VisualFormat};
use latexsnipper_foundation::SnipperError;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::SnipperImage;
#[cfg(target_os = "windows")]
use latexsnipper_runtime::OnnxRuntimeBackend;
use latexsnipper_runtime::StubRuntime;
use std::path::Path;

use crate::{DocumentParseMode, EngineConfig, RecognizeMode, SnipperEngine};

/// Main entry point for LaTeXSnipper SDK.
pub struct Snipper {
    engine: SnipperEngine,
    document: Document,
}

impl Snipper {
    /// Create from a supported file path.
    ///
    /// Image files use OCR for backwards compatibility. Native document and
    /// text formats use the unified importer registry.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SnipperError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| SnipperError::Io(format!("Failed to read '{}': {e}", path.display())))?;
        let format = DocumentImporter::detect_format(&bytes, Some(path))?;

        if !is_raster_image(format) {
            return Self::import_path(path, ImportOptions::default());
        }

        log::info!("Loading image from {:?}", path);

        let img =
            decode(ImageSource::File(path)).map_err(|e| SnipperError::Image(e.to_string()))?;
        let rgb = rgba_to_rgb(&img);
        log::info!("Image loaded: {}x{}", rgb.width(), rgb.height());

        Self::from_image(rgb)
    }

    /// Import a path without invoking OCR.
    pub fn import_path(
        path: impl AsRef<Path>,
        options: ImportOptions,
    ) -> Result<Self, SnipperError> {
        let document = DocumentImporter::from_path(path, options)?;
        Ok(Self::from_imported_document(
            document,
            EngineConfig::default(),
        ))
    }

    /// Import an in-memory document without invoking OCR.
    pub fn from_bytes(
        bytes: &[u8],
        format_hint: Option<InputFormat>,
        options: ImportOptions,
    ) -> Result<Self, SnipperError> {
        let document = DocumentImporter::from_bytes(bytes, format_hint, options)?;
        Ok(Self::from_imported_document(
            document,
            EngineConfig::default(),
        ))
    }

    /// Wrap an existing AST for conversion and export.
    pub fn from_document(document: Document) -> Self {
        Self::from_imported_document(document, EngineConfig::default())
    }

    fn from_imported_document(document: Document, config: EngineConfig) -> Self {
        Self {
            engine: SnipperEngine::new(config, Box::new(StubRuntime::new())),
            document,
        }
    }

    /// Create from a file path using a custom engine config and recognition mode.
    pub fn from_file_with_config(
        path: impl AsRef<Path>,
        config: EngineConfig,
        mode: RecognizeMode,
    ) -> Result<Self, SnipperError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| SnipperError::Io(format!("Failed to read '{}': {e}", path.display())))?;
        let format = DocumentImporter::detect_format(&bytes, Some(path))?;
        if format == InputFormat::Pdf {
            return Self::from_pdf_with_config(path, config, mode);
        }
        if !is_raster_image(format) {
            let document =
                DocumentImporter::from_bytes(&bytes, Some(format), ImportOptions::default())?;
            return Ok(Self::from_imported_document(document, config));
        }

        let img =
            decode(ImageSource::File(path)).map_err(|e| SnipperError::Image(e.to_string()))?;
        Self::from_image_with_config(rgba_to_rgb(&img), config, mode)
    }

    /// Create from a file path using a parse mode and recognition mode.
    pub fn from_file_with_parse_mode(
        path: impl AsRef<Path>,
        parse_mode: DocumentParseMode,
        mode: RecognizeMode,
    ) -> Result<Self, SnipperError> {
        Self::from_file_with_config(
            path,
            EngineConfig::default().set_parse_mode(parse_mode),
            mode,
        )
    }

    /// Create from a PDF file path.
    ///
    /// PDF page rendering requires `pdftoppm` (poppler) or `mutool` (MuPDF).
    pub fn from_pdf(path: impl AsRef<Path>) -> Result<Self, SnipperError> {
        Self::from_pdf_with_config(path, EngineConfig::default(), RecognizeMode::Mixed)
    }

    /// Create from a PDF file path using a custom engine config and recognition mode.
    #[allow(unused_variables)]
    pub fn from_pdf_with_config(
        path: impl AsRef<Path>,
        config: EngineConfig,
        mode: RecognizeMode,
    ) -> Result<Self, SnipperError> {
        #[cfg(target_os = "windows")]
        {
            let backend = OnnxRuntimeBackend::new(config.models_dir.clone())
                .map_err(|e| SnipperError::Runtime(e.to_string()))?;
            let engine = SnipperEngine::new(config, Box::new(backend));

            let rt =
                tokio::runtime::Runtime::new().map_err(|e| SnipperError::Runtime(e.to_string()))?;
            let doc = rt
                .block_on(engine.recognize_pdf(path.as_ref(), mode))
                .map_err(|e| SnipperError::Inference(e.to_string()))?;

            Ok(Self {
                engine,
                document: doc,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(SnipperError::Runtime(
                "PDF processing requires Windows (ONNX Runtime)".to_string(),
            ))
        }
    }

    /// Create from raw RGB pixels.
    pub fn from_image(img: SnipperImage) -> Result<Self, SnipperError> {
        Self::from_image_with_config(img, EngineConfig::default(), RecognizeMode::Formula)
    }

    /// Create from raw RGB pixels using a custom engine config and recognition mode.
    #[allow(unused_variables)]
    pub fn from_image_with_config(
        img: SnipperImage,
        config: EngineConfig,
        mode: RecognizeMode,
    ) -> Result<Self, SnipperError> {
        #[cfg(target_os = "windows")]
        {
            let backend = OnnxRuntimeBackend::new(config.models_dir.clone())
                .map_err(|e| SnipperError::Runtime(e.to_string()))?;
            let engine = SnipperEngine::new(config, Box::new(backend));

            let rt =
                tokio::runtime::Runtime::new().map_err(|e| SnipperError::Runtime(e.to_string()))?;
            let doc = rt
                .block_on(engine.recognize(img, mode))
                .map_err(|e| SnipperError::Inference(e.to_string()))?;

            Ok(Self {
                engine,
                document: doc,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(SnipperError::Runtime(
                "Image processing requires Windows (ONNX Runtime)".to_string(),
            ))
        }
    }

    /// Get the Document AST.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Get a reference to the underlying engine.
    pub fn engine(&self) -> &SnipperEngine {
        &self.engine
    }

    /// Export to LaTeX.
    pub fn to_latex(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to LaTeX");
        DocumentConverter::new(OutputFormat::Latex)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to Markdown.
    pub fn to_markdown(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to Markdown");
        DocumentConverter::new(OutputFormat::MarkdownBlock)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to Typst.
    pub fn to_typst(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to Typst");
        DocumentConverter::new(OutputFormat::Typst)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to HTML.
    pub fn to_html(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to HTML");
        DocumentConverter::new(OutputFormat::Html)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to MathML.
    pub fn to_mathml(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to MathML");
        DocumentConverter::new(OutputFormat::MathML)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to OMML (Office Math Markup Language).
    pub fn to_omml(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to OMML");
        DocumentConverter::new(OutputFormat::OMML)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to JSON.
    pub fn to_json(&self) -> Result<String, SnipperError> {
        log::info!("Exporting to JSON");
        serde_json::to_string_pretty(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export to a specific format.
    pub fn to_format(&self, format: OutputFormat) -> Result<String, SnipperError> {
        log::info!("Exporting to {:?}", format);
        DocumentConverter::new(format)
            .convert(&self.document)
            .map_err(|e| SnipperError::Conversion(e.to_string()))
    }

    /// Export the current document to a visual text or binary artifact.
    pub fn export(&self, format: VisualFormat) -> Result<ExportArtifact, SnipperError> {
        ExportService::export(&self.document, format)
    }

    /// Export through the unified semantic, visual, and package registry.
    pub fn export_format(&self, format: ExportFormat) -> Result<ExportArtifact, SnipperError> {
        DocumentExportService::export(&self.document, format)
    }
}

fn is_raster_image(format: InputFormat) -> bool {
    matches!(
        format,
        InputFormat::ImagePng
            | InputFormat::ImageJpeg
            | InputFormat::ImageWebp
            | InputFormat::ImageBmp
            | InputFormat::ImageTiff
            | InputFormat::ImageGif
    )
}

fn rgba_to_rgb(img: &SnipperImage) -> SnipperImage {
    let mut rgb = Vec::with_capacity((img.width() * img.height() * 3) as usize);
    for chunk in img.pixels().chunks_exact(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }
    SnipperImage::new(img.width(), img.height(), PixelFormat::Rgb, rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_import_and_binary_export_share_the_public_sdk() {
        let snipper = Snipper::from_bytes(
            b"# Title\n\nFormula: $x^2$",
            Some(InputFormat::Markdown),
            ImportOptions::default(),
        )
        .unwrap();
        assert!(snipper.document().block_count() >= 2);

        let artifact = snipper.export(VisualFormat::Png).unwrap();
        assert_eq!(&artifact.as_bytes().unwrap()[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn file_api_no_longer_assumes_non_image_inputs_are_images() {
        let path =
            std::env::temp_dir().join(format!("latexsnipper-sdk-import-{}.md", std::process::id()));
        std::fs::write(&path, "# Imported\n\nText").unwrap();
        let snipper = Snipper::from_file(&path).unwrap();
        std::fs::remove_file(path).ok();
        assert!(snipper.document().block_count() >= 2);
    }
}
