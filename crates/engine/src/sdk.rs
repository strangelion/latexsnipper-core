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
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_foundation::SnipperError;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::OnnxRuntimeBackend;
use std::path::Path;

use crate::{EngineConfig, RecognizeMode, SnipperEngine};

/// Main entry point for LaTeXSnipper SDK.
pub struct Snipper {
    engine: SnipperEngine,
    document: Document,
}

impl Snipper {
    /// Create from an image file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SnipperError> {
        let path = path.as_ref();
        log::info!("Loading image from {:?}", path);

        let img =
            decode(ImageSource::File(path)).map_err(|e| SnipperError::Image(e.to_string()))?;
        let rgb = rgba_to_rgb(&img);
        log::info!("Image loaded: {}x{}", rgb.width(), rgb.height());

        Self::from_image(rgb)
    }

    /// Create from a PDF file path.
    ///
    /// PDF page rendering requires `pdftoppm` (poppler) or `mutool` (MuPDF).
    pub fn from_pdf(path: impl AsRef<Path>) -> Result<Self, SnipperError> {
        let config = EngineConfig::default();
        let backend = OnnxRuntimeBackend::new(config.models_dir.clone())
            .map_err(|e| SnipperError::Runtime(e.to_string()))?;
        let engine = SnipperEngine::new(config, Box::new(backend));

        let rt =
            tokio::runtime::Runtime::new().map_err(|e| SnipperError::Runtime(e.to_string()))?;
        let doc = rt
            .block_on(engine.recognize_pdf(path.as_ref(), RecognizeMode::Mixed))
            .map_err(|e| SnipperError::Inference(e.to_string()))?;

        Ok(Self {
            engine,
            document: doc,
        })
    }

    /// Create from raw RGB pixels.
    pub fn from_image(img: SnipperImage) -> Result<Self, SnipperError> {
        let config = EngineConfig::default();
        let backend = OnnxRuntimeBackend::new(config.models_dir.clone())
            .map_err(|e| SnipperError::Runtime(e.to_string()))?;
        let engine = SnipperEngine::new(config, Box::new(backend));

        let rt =
            tokio::runtime::Runtime::new().map_err(|e| SnipperError::Runtime(e.to_string()))?;
        let doc = rt
            .block_on(engine.recognize(img, RecognizeMode::Formula))
            .map_err(|e| SnipperError::Inference(e.to_string()))?;

        Ok(Self {
            engine,
            document: doc,
        })
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
