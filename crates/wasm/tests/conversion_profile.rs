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

#[wasm_bindgen_test]
fn quoted_unicode_text_keeps_the_formula_only_envelope() {
    for output in ["unicode-math", "omml", "mathml"] {
        let result = json(convert_formula_v3(
            "\"a&100%\"",
            "unicode-math",
            output,
            Some("best-effort".into()),
        ));
        assert_eq!(result["ok"].as_bool(), Some(true));
        let content = result["data"]["content"].as_str().unwrap();
        if output == "unicode-math" {
            assert_eq!(content, "\"a&100%\"");
        } else {
            assert!(content.contains("a&amp;100%"));
        }
    }
}

#[wasm_bindgen_test]
fn unicode_function_arguments_keep_the_formula_only_envelope() {
    let result = json(convert_formula_v3(
        "sin⁡〖x+y〗+b",
        "unicode-math",
        "omml",
        Some("best-effort".into()),
    ));
    assert_eq!(result["ok"].as_bool(), Some(true));
    let content = result["data"]["content"].as_str().unwrap();
    let end = content.find("</m:func>").unwrap();
    assert!(content[..end].contains("<m:t>y</m:t>"));
    assert!(!content[..end].contains("<m:t>b</m:t>"));
    assert!(content[end..].contains("<m:t>b</m:t>"));
}

#[wasm_bindgen_test]
fn identifier_function_names_and_scripts_roundtrip_through_xml() {
    let source = "Foo_i^n⁡〖x+y〗+b";
    let canonical = json(convert_formula_v3(
        source,
        "unicode-math",
        "unicode-math",
        Some("best-effort".into()),
    ));
    assert_eq!(canonical["ok"], true);
    assert_eq!(canonical["data"]["content"], "Foo⁡_(i)^(n) 〖x + y〗 + b");
    for format in ["mathml", "omml"] {
        let xml = json(convert_formula_v3(
            source,
            "unicode-math",
            format,
            Some("best-effort".into()),
        ));
        assert_eq!(xml["ok"], true);
        let restored = json(convert_formula_v3(
            xml["data"]["content"].as_str().unwrap(),
            format,
            "unicode-math",
            Some("best-effort".into()),
        ));
        assert_eq!(restored["ok"], true);
        assert_eq!(restored["data"]["content"], canonical["data"]["content"]);
    }
}

#[wasm_bindgen_test]
fn unicode_nary_scope_keeps_the_formula_only_envelope() {
    let result = json(convert_formula_v3(
        "∫_0^1▒〖x+y〗+b",
        "unicode-math",
        "omml",
        Some("best-effort".into()),
    ));
    assert_eq!(result["ok"].as_bool(), Some(true));
    let content = result["data"]["content"].as_str().unwrap();
    let end = content.find("</m:nary>").unwrap();
    assert!(content[..end].contains("<m:t>y</m:t>"));
    assert!(!content[..end].contains("<m:t>b</m:t>"));
    assert!(content[end..].contains("<m:t>b</m:t>"));
}

#[wasm_bindgen_test]
fn structured_mathml_growth_roundtrips_through_formula_bindings() {
    for (source, stretchy) in [
        ("∫65_0^1▒x+b", "false"),
        ("∑130_i^n▒x+b", "true"),
        ("∫88▒x", "false"),
    ] {
        let canonical = json(convert_formula_v3(
            source,
            "unicode-math",
            "unicode-math",
            Some("best-effort".into()),
        ));
        let mathml = json(convert_formula_v3(
            source,
            "unicode-math",
            "mathml",
            Some("best-effort".into()),
        ));
        assert_eq!(canonical["ok"], true);
        assert_eq!(mathml["ok"], true);
        let xml = mathml["data"]["content"].as_str().unwrap();
        assert!(xml.contains(&format!("stretchy=\"{stretchy}\"")));
        assert!(!xml.contains("LatexSnipperInternal"));
        let restored = json(convert_formula_v3(
            xml,
            "mathml",
            "unicode-math",
            Some("best-effort".into()),
        ));
        assert_eq!(restored["ok"], true);
        assert_eq!(restored["data"]["content"], canonical["data"]["content"]);
        let omml = json(convert_formula_v3(
            xml,
            "mathml",
            "omml",
            Some("best-effort".into()),
        ));
        assert_eq!(omml["ok"], true);
        assert!(omml["data"]["content"].as_str().unwrap().contains(&format!(
            "<m:grow m:val=\"{}\"/>",
            if stretchy == "true" { "1" } else { "0" }
        )));
    }
}
