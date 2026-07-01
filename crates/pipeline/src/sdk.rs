//! LaTeXSnipper Core SDK — One-line Image to Export
//!
//! ```rust,no_run
//! use latexsnipper_pipeline::sdk::Snipper;
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
use latexsnipper_image::image::SnipperImage;
use latexsnipper_inference::{
    detect_formulas, filter_formula_detections, group_formula_detections, recognize_formula,
    DetectionParams, RecognitionParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, OnnxRuntimeBackend, RuntimeBackend};
use std::path::{Path, PathBuf};

/// Main entry point for LaTeXSnipper SDK.
pub struct Snipper {
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

    /// Create from raw RGB pixels.
    pub fn from_image(img: SnipperImage) -> Result<Self, SnipperError> {
        let models = find_models_dir()?;
        log::info!("Using models from {:?}", models);

        let backend = OnnxRuntimeBackend::new(models.clone())
            .map_err(|e| SnipperError::Runtime(e.to_string()))?;

        // 1. Detect formulas
        log::info!("Detecting formulas...");
        let det_config =
            latexsnipper_model::ModelConfig::load(&models.join("formula-det/yolov8-mfd"))
                .map_err(|e| SnipperError::Model(e.to_string()))?;

        let det_params = DetectionParams::from_config(&det_config);
        let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
        let det_handle = ModelHandle::with_path("formula-det", det_path);
        let det_session = backend
            .create_session(&det_handle, AccelerationMode::Cpu)
            .map_err(|e| SnipperError::Runtime(e.to_string()))?;

        let mut detections = detect_formulas(&img, &*det_session, &det_params)
            .map_err(|e| SnipperError::Inference(e.to_string()))?;

        group_formula_detections(&mut detections);
        filter_formula_detections(&mut detections, 100.0, 0.2);

        // Sort by position for reading order
        detections.sort_by(|a, b| {
            a.rect
                .y
                .partial_cmp(&b.rect.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.rect
                        .x
                        .partial_cmp(&b.rect.x)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        log::info!("Detected {} formula regions", detections.len());

        // 2. Recognize formulas
        log::info!("Recognizing formulas...");
        let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
        let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
        let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");

        let enc_handle = ModelHandle::with_path("encoder", enc_path);
        let dec_handle = ModelHandle::with_path("decoder", dec_path);
        let enc_session = backend
            .create_session(&enc_handle, AccelerationMode::Cpu)
            .map_err(|e| SnipperError::Runtime(e.to_string()))?;
        let dec_session = backend
            .create_session(&dec_handle, AccelerationMode::Cpu)
            .map_err(|e| SnipperError::Runtime(e.to_string()))?;

        let rec_params = RecognitionParams::default();
        let mut blocks = Vec::new();

        for det in &detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if w >= 4 && h >= 4 {
                let crop = crop_region(&img, x, y, w, h);
                if let Ok(result) =
                    recognize_formula(&crop, &*enc_session, &*dec_session, &tok_path, &rec_params)
                {
                    log::debug!(
                        "Recognized formula at ({}, {}): {}",
                        x,
                        y,
                        &result.text[..result.text.len().min(50)]
                    );
                    let mut f = Formula::latex(result.text);
                    f.confidence = result.confidence;
                    blocks.push(Block::Formula(FormulaBlock {
                        formula: f,
                        geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                        source: Some(SourceInfo::new()),
                    }));
                }
            }
        }

        // 3. Build Document AST
        log::info!("Building Document AST with {} blocks", blocks.len());
        let doc = Document {
            metadata: Metadata::default(),
            pages: vec![Page {
                width: img.width() as f32,
                height: img.height() as f32,
                blocks,
                page_number: Some(1),
            }],
            id_gen: NodeIdGenerator::new(),
        };

        Ok(Self { document: doc })
    }

    /// Get the Document AST.
    pub fn document(&self) -> &Document {
        &self.document
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

fn find_models_dir() -> Result<PathBuf, SnipperError> {
    let candidates = [
        PathBuf::from("models"),
        PathBuf::from("../models"),
        PathBuf::from("../../models"),
    ];

    for path in &candidates {
        if path
            .join("formula-det/yolov8-mfd/mathcraft-mfd.onnx")
            .exists()
        {
            log::info!("Found models at {:?}", path);
            return Ok(path.clone());
        }
    }

    Err(SnipperError::Model("Models directory not found".into()))
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

fn crop_region(img: &SnipperImage, x: u32, y: u32, w: u32, h: u32) -> SnipperImage {
    let img_w = img.width();
    let img_h = img.height();
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let w = w.min(img_w - x);
    let h = h.min(img_h - y);
    if w == 0 || h == 0 {
        return SnipperImage::new(1, 1, PixelFormat::Rgb, vec![0, 0, 0]);
    }
    let mut pixels = Vec::with_capacity((w * h * 3) as usize);
    for row in 0..h {
        let src_off = ((y + row) * img_w + x) * 3;
        let src_end = src_off + w * 3;
        if src_end as usize <= img.pixels().len() {
            pixels.extend_from_slice(&img.pixels()[src_off as usize..src_end as usize]);
        }
    }
    SnipperImage::new(w, h, PixelFormat::Rgb, pixels)
}
