use latexsnipper_ast::Document;
use latexsnipper_conversion::{
    Converter, HtmlConverter, LatexConverter, MarkdownBlockConverter, MarkdownInlineConverter,
    MathmlConverter, OmmlConverter, TypstConverter,
};
use latexsnipper_engine::{EngineConfig, RecognizeMode, SnipperEngine};
use latexsnipper_image::PixelFormat;
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_syntax::markdown::MarkdownRenderer;
use latexsnipper_syntax::typst::TypstRenderer;
use latexsnipper_syntax::{Parser as _, Renderer as _};
use latexsnipper_tract::TractBackend;
use wasm_bindgen::prelude::*;

mod model_store;

/// Initialize the WASM module.
#[wasm_bindgen]
pub fn init() {
    log::info!("LaTeXSnipper WASM initialized");
}

// ============================================================================
// Model Management
// ============================================================================

/// Load a model from bytes (fetched by JS).
///
/// Models are stored by name and can be retrieved later for inference.
/// Call this after fetching model files with `fetch()` in JavaScript.
#[wasm_bindgen]
pub fn load_model(name: &str, bytes: &[u8]) {
    model_store::store_model(name, bytes.to_vec());
}

/// Check if a model is loaded.
#[wasm_bindgen]
pub fn is_model_loaded(name: &str) -> bool {
    model_store::has_model(name)
}

/// Get list of loaded model names as a JSON array string.
#[wasm_bindgen]
pub fn loaded_models() -> String {
    let models = model_store::list_models();
    serde_json::to_string(&models).unwrap_or_default()
}

// ============================================================================
// Image Recognition
// ============================================================================

/// Recognize an image and return the result as Document JSON.
///
/// # Arguments
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `pixels` - Raw pixel data (RGBA format, 4 bytes per pixel)
/// * `mode` - Recognition mode: "formula", "text", "mixed", "handwriting", "table"
///
/// # Returns
/// JSON string containing the recognized Document.
///
/// # Example (JavaScript)
/// ```js
/// const imageData = canvas.getImageData(0, 0, w, h);
/// const docJson = recognize(w, h, new Uint8Array(imageData.data.buffer), "formula");
/// const latex = convert_document(docJson, "latex");
/// ```
#[wasm_bindgen]
pub fn recognize(width: u32, height: u32, pixels: &[u8], mode: &str) -> Result<String, JsValue> {
    // Create image from RGBA pixels
    let expected_len = (width * height * 4) as usize;
    if pixels.len() != expected_len {
        return Err(JsValue::from_str(&format!(
            "Pixel data length mismatch: expected {} bytes, got {}",
            expected_len,
            pixels.len()
        )));
    }

    let image =
        latexsnipper_image::SnipperImage::new(width, height, PixelFormat::Rgba, pixels.to_vec());
    let recognize_mode = parse_mode(mode)?;

    // Create engine config (models_dir is not used in WASM, models come from store)
    let config = EngineConfig::with_models_dir(std::path::PathBuf::from("/dev/null"));
    let engine = SnipperEngine::new(config, Box::new(TractBackend::new(None)));

    // Run recognition in tokio runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(err_to_js)?;

    let doc = rt
        .block_on(engine.recognize(image, recognize_mode))
        .map_err(err_to_js)?;

    serde_json::to_string(&doc).map_err(err_to_js)
}

/// Recognize an image from a Uint8Array of pixel data.
///
/// This is a convenience wrapper that takes the mode as a string.
#[wasm_bindgen]
pub fn recognize_formula(width: u32, height: u32, pixels: &[u8]) -> Result<String, JsValue> {
    recognize(width, height, pixels, "formula")
}

/// Recognize text in an image.
#[wasm_bindgen]
pub fn recognize_text(width: u32, height: u32, pixels: &[u8]) -> Result<String, JsValue> {
    recognize(width, height, pixels, "text")
}

/// Recognize mixed content (formulas + text) in an image.
#[wasm_bindgen]
pub fn recognize_mixed(width: u32, height: u32, pixels: &[u8]) -> Result<String, JsValue> {
    recognize(width, height, pixels, "mixed")
}

// ============================================================================
// Existing AST/Conversion APIs (unchanged)
// ============================================================================

/// Parse a LaTeX string and return the Document as a JS object.
#[wasm_bindgen]
pub fn parse_latex(latex: &str) -> Result<JsValue, JsValue> {
    let parser = LatexParser;
    let doc = parser.parse(latex).map_err(err_to_js)?;
    to_js_value(&doc)
}

/// Render a Document (as JSON string) to LaTeX string.
#[wasm_bindgen]
pub fn render_latex(doc_json: &str) -> Result<String, JsValue> {
    let doc: Document = serde_json::from_str(doc_json).map_err(err_to_js)?;
    let renderer = LatexRenderer;
    renderer.render(&doc).map_err(err_to_js)
}

/// Render a Document (as JSON string) to Typst string.
#[wasm_bindgen]
pub fn render_typst(doc_json: &str) -> Result<String, JsValue> {
    let doc: Document = serde_json::from_str(doc_json).map_err(err_to_js)?;
    let renderer = TypstRenderer;
    renderer.render(&doc).map_err(err_to_js)
}

/// Render a Document (as JSON string) to Markdown string.
#[wasm_bindgen]
pub fn render_markdown(doc_json: &str) -> Result<String, JsValue> {
    let doc: Document = serde_json::from_str(doc_json).map_err(err_to_js)?;
    let renderer = MarkdownRenderer;
    renderer.render(&doc).map_err(err_to_js)
}

/// Convert a Document JSON to the specified format.
#[wasm_bindgen]
pub fn convert_document(doc_json: &str, format: &str) -> Result<String, JsValue> {
    let doc: Document = serde_json::from_str(doc_json).map_err(err_to_js)?;

    let result = match format {
        "latex" => LatexConverter.convert(&doc),
        "latex_display" => latexsnipper_conversion::LatexDisplayConverter.convert(&doc),
        "latex_equation" => latexsnipper_conversion::LatexEquationConverter.convert(&doc),
        "markdown_inline" => MarkdownInlineConverter.convert(&doc),
        "markdown_block" => MarkdownBlockConverter.convert(&doc),
        "mathml" => MathmlConverter.convert(&doc),
        "omml" => OmmlConverter.convert(&doc),
        "typst" => TypstConverter.convert(&doc),
        "html" => HtmlConverter.convert(&doc),
        _ => return Err(JsValue::from_str(&format!("Unknown format: {}", format))),
    };

    result.map_err(err_to_js)
}

/// Get the LaTeX string from a formula JSON.
#[wasm_bindgen]
pub fn formula_to_latex(formula_json: &str) -> Result<String, JsValue> {
    let formula: latexsnipper_ast::Formula =
        serde_json::from_str(formula_json).map_err(err_to_js)?;
    Ok(formula.as_latex().to_string())
}

/// Get available conversion formats as a JSON array string.
#[wasm_bindgen]
pub fn available_formats() -> String {
    let formats = vec![
        "latex",
        "latex_display",
        "latex_equation",
        "markdown_inline",
        "markdown_block",
        "mathml",
        "omml",
        "typst",
        "html",
    ];
    serde_json::to_string(&formats).unwrap_or_default()
}

/// Build a Document from JSON and export to the specified format.
/// This is the main "AST -> Export" function for WASM.
#[wasm_bindgen]
pub fn convert_from_json(doc_json: &str, format: &str) -> Result<String, JsValue> {
    convert_document(doc_json, format)
}

/// Create a Document with a formula and export to format.
/// Convenience function for simple use cases.
#[wasm_bindgen]
pub fn formula_to_document(latex: &str, format: &str) -> Result<String, JsValue> {
    let doc = Document {
        metadata: latexsnipper_ast::Metadata::default(),
        pages: vec![latexsnipper_ast::Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![latexsnipper_ast::Block::Formula(
                latexsnipper_ast::FormulaBlock {
                    formula: latexsnipper_ast::Formula::latex(latex),
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: None,
                },
            )],
            page_number: Some(1),
            layout: None,
            background_asset_id: None,
        }],
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
    };

    let doc_json = serde_json::to_string(&doc).map_err(err_to_js)?;
    convert_document(&doc_json, format)
}

/// Check if the WASM module is working.
#[wasm_bindgen]
pub fn health_check() -> String {
    "ok".to_string()
}

/// Get module version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_mode(mode: &str) -> Result<RecognizeMode, JsValue> {
    match mode.to_lowercase().as_str() {
        "formula" | "f" => Ok(RecognizeMode::Formula),
        "text" | "t" => Ok(RecognizeMode::Text),
        "mixed" | "m" => Ok(RecognizeMode::Mixed),
        "handwriting" | "hw" => Ok(RecognizeMode::Handwriting),
        "table" | "tbl" => Ok(RecognizeMode::Table),
        "formula_layout" | "fl" => Ok(RecognizeMode::FormulaLayout),
        _ => Err(JsValue::from_str(&format!(
            "Unknown recognition mode: '{}'. Use 'formula', 'text', 'mixed', 'handwriting', or 'table'.",
            mode
        ))),
    }
}

fn err_to_js<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn to_js_value<T: serde::Serialize>(v: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(v).map_err(|e| JsValue::from_str(&e.to_string()))
}
