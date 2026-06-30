use wasm_bindgen::prelude::*;
use latexsnipper_ast::Document;
use latexsnipper_syntax::{Parser as _, Renderer as _};
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_syntax::typst::TypstRenderer;
use latexsnipper_syntax::markdown::MarkdownRenderer;
use latexsnipper_conversion::{Converter, LatexConverter, OmmlConverter, MathmlConverter, TypstConverter, MarkdownInlineConverter, MarkdownBlockConverter, HtmlConverter};
use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_runtime::StubRuntime;

/// Initialize the WASM module.
#[wasm_bindgen]
pub fn init() {
    log::info!("LaTeXSnipper WASM initialized");
}

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
    let formula: latexsnipper_ast::Formula = serde_json::from_str(formula_json).map_err(err_to_js)?;
    Ok(formula.as_latex().to_string())
}

/// Get available conversion formats as a JSON array string.
#[wasm_bindgen]
pub fn available_formats() -> String {
    let formats = vec![
        "latex", "latex_display", "latex_equation",
        "markdown_inline", "markdown_block",
        "mathml", "omml", "typst", "html",
    ];
    serde_json::to_string(&formats).unwrap_or_default()
}

/// Recognize content in image data (raw RGB bytes).
/// Returns Document JSON string.
/// mode: "formula", "text", or "mixed"
#[wasm_bindgen]
pub fn recognize(data: &[u8], width: u32, height: u32, mode: &str) -> Result<String, JsValue> {
    use latexsnipper_image::SnipperImage;
    use latexsnipper_image::color::PixelFormat;

    if data.len() != (width * height * 3) as usize {
        return Err(JsValue::from_str("Data length doesn't match width*height*3"));
    }

    let image = SnipperImage::new(width, height, PixelFormat::Rgb, data.to_vec());

    let recognize_mode = match mode {
        "text" => RecognizeMode::Text,
        "mixed" => RecognizeMode::Mixed,
        _ => RecognizeMode::Formula,
    };

    let config = EngineConfig::default();
    let runtime = Box::new(StubRuntime::new());
    let engine = SnipperEngine::new(config, runtime);

    // Note: WASM is single-threaded, so we use a basic tokio runtime
    let rt = tokio::runtime::Runtime::new().map_err(|e| JsValue::from_str(&e.to_string()))?;
    let doc = rt.block_on(engine.recognize(image, recognize_mode))
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    serde_json::to_string(&doc).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Check if the WASM module is working.
#[wasm_bindgen]
pub fn health_check() -> String {
    "ok".to_string()
}

fn err_to_js<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn to_js_value<T: serde::Serialize>(v: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(v).map_err(|e| JsValue::from_str(&e.to_string()))
}
