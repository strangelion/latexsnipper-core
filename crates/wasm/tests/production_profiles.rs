use std::path::PathBuf;

use latexsnipper_image::{PixelFormat, SnipperImage};
use latexsnipper_inference::{
    build_grid_from_detections, recognize_formula_with_tokenizer,
    recognize_table_transformer_with_max_edge, RecognitionParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend};
use latexsnipper_tensor::Tensor;
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
fn production_formula_detector_artifact_decodes() {
    let Some(model_path) = require(models_dir().join("formula-det/yolov8-mfd/mathcraft-mfd.onnx"))
    else {
        return;
    };
    TractBackend::validate_model_bytes(&std::fs::read(model_path).unwrap()).unwrap();
}

#[test]
#[ignore = "the 80 MB detector's full Tract session is an opt-in performance test"]
fn production_formula_detector_executes_with_the_wasm_runtime() {
    let Some(model_path) = require(models_dir().join("formula-det/yolov8-mfd/mathcraft-mfd.onnx"))
    else {
        return;
    };
    let backend = TractBackend::new(None);
    let load_started = std::time::Instant::now();
    eprintln!("loading production formula detector with Tract");
    let session = backend
        .create_session(
            &ModelHandle::with_bytes("formula-det/yolov8-mfd", std::fs::read(model_path).unwrap())
                .with_input_shape(vec![1, 3, 768, 768]),
            AccelerationMode::Cpu,
        )
        .unwrap();
    eprintln!("formula detector loaded in {:?}", load_started.elapsed());
    let side = 768usize;
    let inference_started = std::time::Instant::now();
    let outputs = session
        .run(&[Tensor::float32(
            "images",
            vec![1, 3, side, side],
            vec![1.0; 3 * side * side],
        )])
        .unwrap();
    eprintln!(
        "formula detector inference completed in {:?}",
        inference_started.elapsed()
    );
    assert_eq!(outputs.len(), 1);
    assert!(!outputs[0].shape().is_empty());
    assert!(outputs[0]
        .as_f32_slice()
        .is_some_and(|values| !values.is_empty() && values.iter().all(|value| value.is_finite())));
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
