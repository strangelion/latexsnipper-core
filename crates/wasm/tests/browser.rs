use js_sys::{Reflect, JSON};
use latexsnipper_wasm::{
    api_info_v3, begin_model_update_v2, capabilities_v2, capabilities_v3, clear_models_v2,
    commit_model_update_v2, convert_v2, convert_v3, load_model_v2, recognize_v2,
    rollback_model_update_v2, unload_model_v2,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn field(value: &JsValue, name: &str) -> JsValue {
    Reflect::get(value, &JsValue::from_str(name)).unwrap()
}

fn error_code(value: &JsValue) -> String {
    field(&field(value, "error"), "code").as_string().unwrap()
}

fn json(value: &JsValue) -> String {
    JSON::stringify(value).unwrap().as_string().unwrap()
}

fn bordered_table_pixels(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = vec![255; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            if x == 0 || x + 1 == width || y == 0 || y + 1 == height {
                let offset = (y * width + x) * 4;
                pixels[offset..offset + 3].fill(0);
            }
        }
    }
    pixels
}

fn load_text_profile() {
    let artifacts: [(&str, &[u8]); 5] = [
        (
            "text-det/tiny/config.json",
            include_bytes!("fixtures/tiny-text-det.json"),
        ),
        (
            "text-det/tiny/model.onnx",
            include_bytes!("fixtures/tiny-text-det.onnx"),
        ),
        (
            "text-rec/tiny/config.json",
            include_bytes!("fixtures/tiny-text-rec.json"),
        ),
        (
            "text-rec/tiny/model.onnx",
            include_bytes!("fixtures/tiny-text-rec.onnx"),
        ),
        (
            "text-rec/tiny/keys.txt",
            include_bytes!("fixtures/keys.txt"),
        ),
    ];
    for (name, bytes) in artifacts {
        let response = load_model_v2(name, bytes.to_vec(), None);
        assert!(
            field(&response, "ok").as_bool().unwrap(),
            "failed to load {name}: {}",
            json(&response)
        );
    }
}

fn load_handwriting_profile() {
    let artifacts: [(&str, &[u8]); 4] = [
        (
            "formula-rec/tiny/config.json",
            include_bytes!("fixtures/tiny-formula-rec.json"),
        ),
        (
            "formula-rec/tiny/encoder.onnx",
            include_bytes!("fixtures/tiny-formula-encoder.onnx"),
        ),
        (
            "formula-rec/tiny/decoder.onnx",
            include_bytes!("fixtures/tiny-formula-decoder.onnx"),
        ),
        (
            "formula-rec/tiny/tokenizer.json",
            include_bytes!("fixtures/tiny-formula-tokenizer.json"),
        ),
    ];
    for (name, bytes) in artifacts {
        let response = load_model_v2(name, bytes.to_vec(), None);
        assert!(
            field(&response, "ok").as_bool().unwrap(),
            "failed to load {name}: {}",
            json(&response)
        );
    }
}

#[wasm_bindgen_test]
fn v3_contract_exports_are_callable_and_versioned_independently() {
    let info = api_info_v3();
    assert!(field(&info, "ok").as_bool().unwrap());
    let versions = field(&info, "versions");
    assert_eq!(field(&versions, "apiEnvelopeVersion").as_f64(), Some(3.0));
    assert_eq!(
        field(&versions, "capabilitySchemaVersion").as_f64(),
        Some(3.0)
    );
    let capability = capabilities_v3();
    assert!(field(&capability, "ok").as_bool().unwrap());
    assert_eq!(
        field(&field(&capability, "data"), "schemaVersion").as_f64(),
        Some(3.0)
    );

    let failure = convert_v3("not-json", "latex");
    assert_eq!(field(&failure, "ok").as_bool(), Some(false));
    assert!(!field(&field(&failure, "error"), "code")
        .as_string()
        .unwrap()
        .is_empty());
    assert!(Reflect::get(&failure, &JsValue::from_str("data"))
        .unwrap()
        .is_undefined());
}

#[wasm_bindgen_test]
fn model_transactions_are_atomic_and_reversible() {
    clear_models_v2();
    assert!(field(&begin_model_update_v2(), "ok").as_bool().unwrap());
    let loaded = load_model_v2("text-det/test/config.json", b"{}".to_vec(), None);
    assert!(field(&loaded, "ok").as_bool().unwrap());
    assert!(field(&loaded, "data").is_object());
    assert!(field(&rollback_model_update_v2(), "ok").as_bool().unwrap());

    let capabilities = capabilities_v2();
    assert!(field(&capabilities, "ok").as_bool().unwrap());
    let cancellation = field(&field(&capabilities, "data"), "cancellation");
    assert!(field(&cancellation, "supported").as_bool().unwrap());
    assert_eq!(
        field(&cancellation, "mode").as_string().unwrap(),
        "cooperative-stage-boundary"
    );
    assert!(!field(&cancellation, "canInterruptActiveInference")
        .as_bool()
        .unwrap());

    assert!(field(&begin_model_update_v2(), "ok").as_bool().unwrap());
    load_model_v2("text-det/test/config.json", b"{}".to_vec(), None);
    assert!(field(&commit_model_update_v2(), "ok").as_bool().unwrap());
    assert!(field(&unload_model_v2("text-det/test/config.json"), "data")
        .as_bool()
        .unwrap());
}

#[wasm_bindgen_test]
fn invalid_model_and_checksum_return_stable_codes() {
    clear_models_v2();
    let invalid = load_model_v2("model.onnx", vec![0, 1, 2], None);
    assert_eq!(error_code(&invalid), "MODEL_ARTIFACT_INVALID");

    let mismatch = load_model_v2("config.json", b"{}".to_vec(), Some("00".to_string()));
    assert_eq!(error_code(&mismatch), "MODEL_CHECKSUM_MISMATCH");
}

#[wasm_bindgen_test(async)]
async fn async_recognition_rejects_invalid_pixels_and_missing_profiles() {
    clear_models_v2();
    let invalid = recognize_v2(2, 2, vec![0; 15], "formula".to_string()).await;
    assert_eq!(error_code(&invalid), "INVALID_IMAGE");

    let missing = recognize_v2(1, 1, vec![0; 4], "formula".to_string()).await;
    assert_eq!(error_code(&missing), "MODEL_ARTIFACT_MISSING");
}

#[wasm_bindgen_test(async)]
async fn tiny_models_run_through_tract_pipeline_ast_and_latex() {
    clear_models_v2();
    load_text_profile();

    let capabilities = json(&capabilities_v2());
    assert!(
        capabilities.contains(r#""profile":"text","ready":true"#),
        "text capability did not become ready: {capabilities}"
    );

    let recognized = recognize_v2(16, 8, vec![255; 16 * 8 * 4], "text".to_string()).await;
    assert!(
        field(&recognized, "ok").as_bool().unwrap(),
        "recognition failed: {}",
        json(&recognized)
    );
    let document_json = json(&field(&recognized, "data"));
    assert!(
        document_json.contains("AB"),
        "missing CTC output: {document_json}"
    );

    let converted = convert_v2(&document_json, "latex");
    assert!(
        field(&converted, "ok").as_bool().unwrap(),
        "conversion failed: {}",
        json(&converted)
    );
    assert!(field(&field(&converted, "data"), "text")
        .as_string()
        .unwrap()
        .contains("AB"));

    let converted_v3 = convert_v3(&document_json, "latex");
    assert!(field(&converted_v3, "ok").as_bool().unwrap());
    assert_eq!(
        field(&field(&converted_v3, "versions"), "apiEnvelopeVersion").as_f64(),
        Some(3.0)
    );
    assert!(field(&field(&converted_v3, "data"), "text")
        .as_string()
        .unwrap()
        .contains("AB"));
}

#[wasm_bindgen_test(async)]
async fn table_profile_runs_projection_structure_and_cell_ocr() {
    clear_models_v2();
    load_text_profile();

    let capabilities = json(&capabilities_v2());
    assert!(
        capabilities.contains(r#""profile":"table","ready":true"#),
        "table capability did not become ready: {capabilities}"
    );
    assert!(capabilities.contains("table-struct/projection"));

    let recognized = recognize_v2(16, 8, bordered_table_pixels(16, 8), "table".to_string()).await;
    assert!(
        field(&recognized, "ok").as_bool().unwrap(),
        "table recognition failed: {}",
        json(&recognized)
    );
    let document_json = json(&field(&recognized, "data"));
    assert!(
        document_json.to_ascii_lowercase().contains("table"),
        "missing table block: {document_json}"
    );
    assert!(
        document_json.contains("AB"),
        "missing cell OCR: {document_json}"
    );
    assert!(
        document_json.contains("confidence"),
        "missing OCR confidence: {document_json}"
    );
}

#[wasm_bindgen_test(async)]
async fn handwriting_profile_runs_encoder_decoder_and_ast() {
    clear_models_v2();
    load_handwriting_profile();

    let capabilities = json(&capabilities_v2());
    assert!(
        capabilities.contains(r#""profile":"handwriting","ready":true"#),
        "handwriting capability did not become ready: {capabilities}"
    );

    let recognized = recognize_v2(8, 8, vec![255; 8 * 8 * 4], "handwriting".to_string()).await;
    assert!(
        field(&recognized, "ok").as_bool().unwrap(),
        "handwriting recognition failed: {}",
        json(&recognized)
    );
    let document_json = json(&field(&recognized, "data"));
    assert!(document_json.to_ascii_lowercase().contains("handwriting"));
    assert!(document_json.contains('A'));
}
