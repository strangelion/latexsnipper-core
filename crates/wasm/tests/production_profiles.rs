use std::path::PathBuf;

use latexsnipper_image::{PixelFormat, SnipperImage};
use latexsnipper_inference::{
    build_grid_from_detections, recognize_formula_with_tokenizer,
    recognize_table_transformer_with_max_edge, RecognitionParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend};
use latexsnipper_tract::TractBackend;

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models")
}

fn require(path: PathBuf) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path);
    }
    assert!(
        std::env::var_os("LATEXSNIPPER_REQUIRE_REAL_MODELS").is_none(),
        "required browser production-profile artifact is missing: {}",
        path.display()
    );
    None
}

#[test]
#[ignore = "TATR is Tract-compatible but exceeds the browser hard-timeout budget"]
fn production_tatr_profile_executes_with_the_wasm_runtime() {
    let Some(model_path) =
        require(models_dir().join("table-struct/tatr-structure/model.browser.onnx"))
    else {
        return;
    };
    let bytes = std::fs::read(model_path).unwrap();
    let backend = TractBackend::new(None);
    let session = backend
        .create_session(
            &ModelHandle::with_bytes("table-struct/tatr-structure", bytes),
            AccelerationMode::Cpu,
        )
        .unwrap();
    let image = SnipperImage::new(64, 64, PixelFormat::Rgb, vec![255; 64 * 64 * 3]);
    let detections = recognize_table_transformer_with_max_edge(&image, &*session, 256).unwrap();
    let cells =
        build_grid_from_detections(&detections, image.width() as f32, image.height() as f32);
    assert!(cells.len() <= 1_024);
}

#[test]
fn production_handwriting_profile_executes_with_the_wasm_runtime() {
    let root = models_dir().join("formula-rec/trocr-deit");
    let Some(encoder_path) = require(root.join("encoder_model.onnx")) else {
        return;
    };
    let Some(decoder_path) = require(root.join("decoder_model.onnx")) else {
        return;
    };
    let Some(tokenizer_path) = require(root.join("tokenizer.json")) else {
        return;
    };
    let backend = TractBackend::new(None);
    let encoder = backend
        .create_session(
            &ModelHandle::with_bytes(
                "formula-rec/trocr-deit/encoder",
                std::fs::read(encoder_path).unwrap(),
            ),
            AccelerationMode::Cpu,
        )
        .unwrap();
    let decoder = backend
        .create_session(
            &ModelHandle::with_bytes(
                "formula-rec/trocr-deit/decoder",
                std::fs::read(decoder_path).unwrap(),
            ),
            AccelerationMode::Cpu,
        )
        .unwrap();
    let tokenizer = std::fs::read_to_string(tokenizer_path).unwrap();
    let vocabulary = latexsnipper_inference::load_tokenizer_from_str(&tokenizer).unwrap();
    assert!(!vocabulary.is_empty());
    let image = SnipperImage::new(64, 64, PixelFormat::Rgb, vec![255; 64 * 64 * 3]);
    let result = recognize_formula_with_tokenizer(
        &image,
        &*encoder,
        &*decoder,
        &vocabulary,
        &RecognitionParams {
            max_tokens: 1,
            ..RecognitionParams::default()
        },
    )
    .unwrap();
    assert!(result.confidence.is_finite());
    assert!((0.0..=1.0).contains(&result.confidence));
}
