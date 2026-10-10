#![cfg(target_arch = "wasm32")]
use latexsnipper_wasm::{api_info_v3, convert_formula_fragment_v3, convert_formula_v3};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn json(value: JsValue) -> serde_json::Value {
    serde_wasm_bindgen::from_value(value).unwrap()
}

#[wasm_bindgen_test]
fn profile_truth_and_formula_contract_do_not_require_recognition_state() {
    let info = json(api_info_v3());
    assert_eq!(info["ok"].as_bool(), Some(true));
    assert_eq!(
        info["data"]["v2CompatibilityExports"].as_bool(),
        Some(cfg!(feature = "recognition"))
    );
    let result = json(convert_formula_fragment_v3(
        "frac(a,b)",
        "typst",
        Some("best-effort".into()),
    ));
    assert_eq!(result["ok"].as_bool(), Some(true));
    let data = &result["data"];
    assert_eq!(data["content"].as_str(), Some("\\frac{a}{b}"));
    assert_eq!(data["contentKind"].as_str(), Some("latex-fragment"));
    let inline = json(convert_formula_v3(
        "frac(a,b)",
        "typst",
        "markdown_inline",
        Some("best-effort".into()),
    ));
    assert_eq!(inline["data"]["content"].as_str(), Some("$\\frac{a}{b}$"));
    let rejected = json(convert_formula_fragment_v3(
        "\\documentclass{article}x",
        "latex",
        Some("best-effort".into()),
    ));
    assert_eq!(rejected["ok"].as_bool(), Some(false));
}
