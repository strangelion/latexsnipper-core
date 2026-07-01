use wasm_bindgen::prelude::*;
use latexsnipper_ast::Document;
use latexsnipper_syntax::{Parser as _, Renderer as _};
use latexsnipper_syntax::latex::{LatexParser, LatexRenderer};
use latexsnipper_syntax::typst::TypstRenderer;
use latexsnipper_syntax::markdown::MarkdownRenderer;
use latexsnipper_conversion::{Converter, LatexConverter, OmmlConverter, MathmlConverter, TypstConverter, MarkdownInlineConverter, MarkdownBlockConverter, HtmlConverter};

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

/// Build a Document from JSON and export to the specified format.
/// This is the main "AST → Export" function for WASM.
///
/// Usage:
/// ```js
/// const doc = { pages: [{ blocks: [{ type: "Formula", formula: { source: { format: "Latex", content: "E=mc^2" } } }] }] };
/// const latex = convert_document(JSON.stringify(doc), "latex");
/// ```
#[wasm_bindgen]
pub fn convert_from_json(doc_json: &str, format: &str) -> Result<String, JsValue> {
    convert_document(doc_json, format)
}

/// Create a Document with a formula and export to format.
/// Convenience function for simple use cases.
///
/// Usage:
/// ```js
/// const latex = formula_to_document("E = mc^2", "latex");
/// const md = formula_to_document("\\frac{a}{b}", "markdown_block");
/// ```
#[wasm_bindgen]
pub fn formula_to_document(latex: &str, format: &str) -> Result<String, JsValue> {
    let doc = Document {
        metadata: latexsnipper_ast::Metadata::default(),
        pages: vec![latexsnipper_ast::Page {
            width: 800.0,
            height: 600.0,
            blocks: vec![latexsnipper_ast::Block::Formula(latexsnipper_ast::FormulaBlock {
                formula: latexsnipper_ast::Formula::latex(latex),
                geometry: None,
                source: None,
            })],
            page_number: Some(1),
        }],
        id_gen: latexsnipper_ast::NodeIdGenerator::new(),
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

fn err_to_js<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn to_js_value<T: serde::Serialize>(v: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(v).map_err(|e| JsValue::from_str(&e.to_string()))
}
