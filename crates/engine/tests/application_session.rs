use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use latexsnipper_ast::{ImportOptions, InputFormat};
use latexsnipper_engine::application::{
    ApplicationErrorCode, CancellationToken, ProgressEvent, ProgressSink, ProgressStage,
    RecognitionControl, RecognitionProfile, RecognitionRequest, RecognitionSession,
};
use latexsnipper_engine::{EngineConfig, SnipperEngine};
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::decode::encode_png;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::StubRuntime;
use latexsnipper_runtime::{AccelerationMode, InferenceSession, ModelHandle, RuntimeBackend};
use latexsnipper_tensor::Tensor;

fn blank_image() -> SnipperImage {
    SnipperImage::new(8, 8, PixelFormat::Rgba, vec![255; 8 * 8 * 4])
}

fn assert_send<T: Send>() {}

#[test]
fn session_is_send_and_serialized_by_mutable_access() {
    assert_send::<RecognitionSession>();
}

#[test]
fn application_error_wire_code_is_stable_and_omits_the_source() {
    let token = CancellationToken::new();
    token.cancel();
    let (_models, mut session) = session();
    let error = session
        .recognize_with_control(
            RecognitionRequest::from_image(blank_image()),
            RecognitionControl {
                progress: None,
                cancellation: Some(token),
            },
        )
        .unwrap_err();
    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["code"], "CANCELLED");
    assert!(json.get("source").is_none());
}

fn session() -> (tempfile::TempDir, RecognitionSession) {
    let models = tempfile::tempdir().unwrap();
    let config = EngineConfig {
        models_dir: models.path().to_path_buf(),
        ..EngineConfig::default()
    };
    let engine = SnipperEngine::new(config, Box::new(StubRuntime::new()));
    let session = RecognitionSession::from_engine(engine).unwrap();
    (models, session)
}

fn write_stub_formula_models(root: &std::path::Path) {
    let detection = root.join("formula-detection").join("stub-det");
    let recognition = root.join("formula-recognition").join("stub-rec");
    std::fs::create_dir_all(&detection).unwrap();
    std::fs::create_dir_all(&recognition).unwrap();
    std::fs::write(
        detection.join("manifest.toml"),
        r#"
id = "formula-detection/stub-det"
task = "FormulaDetection"
version = "1.0.0"
adapter = "yolov8-detection-v1"

[input]
name = "images"
shape = [1, 3, 640, 640]
dtype = "float32"

[[output]]
name = "output"
shape = [1, 8400, 6]
dtype = "float32"

[[runtimeVariants]]
id = "stub"
runtime = "stub"
status = "stable"
priority = 100

[runtimeVariants.artifacts]
model = "model.onnx"
"#,
    )
    .unwrap();
    std::fs::write(detection.join("model.onnx"), b"stub").unwrap();

    std::fs::write(
        recognition.join("manifest.toml"),
        r#"
id = "formula-recognition/stub-rec"
task = "FormulaRecognition"
version = "1.0.0"
adapter = "onnx-formula-v1"

[input]
name = "image"
shape = [1, 3, 384, 384]
dtype = "float32"

[[output]]
name = "logits"
shape = [1, -1, -1]
dtype = "float32"

[[runtimeVariants]]
id = "stub"
runtime = "stub"
status = "stable"
priority = 100

[runtimeVariants.artifacts]
model = "encoder.onnx"
"#,
    )
    .unwrap();
    std::fs::write(recognition.join("encoder.onnx"), b"stub").unwrap();
    std::fs::write(recognition.join("decoder.onnx"), b"stub").unwrap();
    std::fs::write(
        recognition.join("config.json"),
        r#"{"model_type":"onnx_formula"}"#,
    )
    .unwrap();
    std::fs::write(recognition.join("vocab.txt"), "<pad>\n<s>\n</s>\nx").unwrap();
}

#[test]
fn session_reuses_engine_and_recovers_after_input_error() {
    let (_models, mut session) = session();
    let engine_address = session.engine() as *const SnipperEngine;

    let error = session
        .recognize(RecognitionRequest::from_bytes(
            b"not an image".to_vec(),
            None,
        ))
        .unwrap_err();
    assert!(matches!(
        error.code,
        ApplicationErrorCode::InvalidInput
            | ApplicationErrorCode::UnsupportedFormat
            | ApplicationErrorCode::ImageDecodeFailed
    ));

    let result = session
        .recognize(RecognitionRequest::from_image(blank_image()))
        .unwrap();
    assert_eq!(result.document.pages.len(), 1);
    assert_eq!(engine_address, session.engine() as *const SnipperEngine);
}

#[test]
fn path_bytes_image_and_unicode_path_share_the_same_session() {
    let (_models, mut session) = session();
    let png = encode_png(&blank_image()).unwrap();
    let input_dir = tempfile::tempdir().unwrap();
    let unicode_path = input_dir.path().join("公式-测试.png");
    std::fs::write(&unicode_path, &png).unwrap();

    for request in [
        RecognitionRequest::from_path(&unicode_path),
        RecognitionRequest::from_bytes(png, Some(InputFormat::ImagePng)),
        RecognitionRequest::from_image(blank_image()),
    ] {
        let result = session.recognize(request).unwrap();
        assert_eq!(result.metadata.image_size, Some((8, 8)));
        assert_eq!(result.document.pages.len(), 1);
    }
}

#[test]
fn damaged_unsupported_and_oversized_inputs_are_structured() {
    let models = tempfile::tempdir().unwrap();
    let engine = SnipperEngine::new(
        EngineConfig {
            models_dir: models.path().to_path_buf(),
            ..EngineConfig::default()
        },
        Box::new(StubRuntime::new()),
    );
    let limits = ImportOptions {
        max_image_width: 4,
        ..ImportOptions::default()
    };
    let mut session = RecognitionSession::from_engine_with_import_options(engine, limits).unwrap();

    let unsupported = session
        .recognize(RecognitionRequest::from_bytes(
            b"plain text".to_vec(),
            Some(InputFormat::PlainText),
        ))
        .unwrap_err();
    assert!(matches!(
        unsupported.code,
        ApplicationErrorCode::InvalidInput | ApplicationErrorCode::UnsupportedFormat
    ));

    let damaged = session
        .recognize(RecognitionRequest::from_bytes(
            b"\x89PNG\r\n\x1a\nbroken".to_vec(),
            Some(InputFormat::ImagePng),
        ))
        .unwrap_err();
    assert_eq!(damaged.code, ApplicationErrorCode::ImageDecodeFailed);

    let missing = session
        .recognize(RecognitionRequest::from_path(
            models.path().join("does-not-exist.png"),
        ))
        .unwrap_err();
    assert_eq!(missing.code, ApplicationErrorCode::InvalidInput);

    let oversized = session
        .recognize(RecognitionRequest::from_image(blank_image()))
        .unwrap_err();
    assert_eq!(oversized.code, ApplicationErrorCode::InputTooLarge);
}

#[test]
fn warmup_is_idempotent_and_profile_switch_keeps_prior_state() {
    let (_models, mut session) = session();
    let first = session.warmup(RecognitionProfile::Formula).unwrap();
    assert!(!first.already_warm);
    let second = session.warmup(RecognitionProfile::Formula).unwrap();
    assert!(second.already_warm);
    assert_eq!(second.elapsed, std::time::Duration::ZERO);

    let text = session.warmup(RecognitionProfile::Text).unwrap();
    assert!(!text.already_warm);
    assert!(
        session
            .warmup(RecognitionProfile::Formula)
            .unwrap()
            .already_warm
    );
}

#[test]
fn warmup_success_reports_resources_that_were_actually_prepared() {
    let models = tempfile::tempdir().unwrap();
    write_stub_formula_models(models.path());
    let engine = SnipperEngine::new(
        EngineConfig {
            models_dir: models.path().to_path_buf(),
            formula_det_model: Some("formula-detection/stub-det".to_string()),
            formula_rec_model: Some("formula-recognition/stub-rec".to_string()),
            ..EngineConfig::default()
        },
        Box::new(StubRuntime::new()),
    );
    let mut session = RecognitionSession::from_engine(engine).unwrap();

    let report = session.warmup(RecognitionProfile::Formula).unwrap();
    assert!(report.ready, "{:?}", report.diagnostics);
    assert_eq!(report.loaded_models.len(), 2);
    assert!(report.missing_models.is_empty());
}

#[derive(Default)]
struct RecordingProgress {
    stages: Mutex<Vec<ProgressStage>>,
}

impl ProgressSink for RecordingProgress {
    fn report(&self, event: ProgressEvent) {
        self.stages.lock().unwrap().push(event.stage);
    }
}

#[test]
fn progress_is_ordered_and_cancellation_does_not_poison_session() {
    let (_models, mut session) = session();
    let sink = Arc::new(RecordingProgress::default());
    let result = session
        .recognize_with_control(
            RecognitionRequest::from_image(blank_image()),
            RecognitionControl {
                progress: Some(sink.clone()),
                cancellation: None,
            },
        )
        .unwrap();
    assert_eq!(result.document.pages.len(), 1);
    let stages = sink.stages.lock().unwrap();
    assert_eq!(stages.first(), Some(&ProgressStage::DecodingInput));
    assert_eq!(stages.last(), Some(&ProgressStage::Completed));
    drop(stages);

    let token = CancellationToken::new();
    token.cancel();
    let error = session
        .recognize_with_control(
            RecognitionRequest::from_image(blank_image()),
            RecognitionControl {
                progress: None,
                cancellation: Some(token),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, ApplicationErrorCode::Cancelled);

    assert!(session
        .recognize(RecognitionRequest::from_image(blank_image()))
        .is_ok());
}

#[test]
fn health_conversions_and_close_have_stable_semantics() {
    let (_models, mut session) = session();
    let health = session.health_check().unwrap();
    assert!(!health.models.is_empty());
    assert!(!session.capabilities().profiles.is_empty());
    assert!(!session.runtime_status().runtimes.is_empty());

    let result = session
        .recognize(RecognitionRequest::from_image(blank_image()))
        .unwrap();
    assert!(result.to_latex().is_ok());
    assert!(result.to_markdown().is_ok());
    assert!(result.to_typst().is_ok());
    assert!(result.to_omml().is_ok());
    assert_eq!(
        serde_json::to_value(&result.diagnostics).unwrap(),
        serde_json::to_value(&result.document.diagnostics).unwrap()
    );

    session.close();
    assert!(session.is_closed());
    session.close();
    let error = session.health_check().unwrap_err();
    assert_eq!(error.code, ApplicationErrorCode::InvalidInput);
}

struct UnavailableRuntime;

impl RuntimeBackend for UnavailableRuntime {
    fn create_session(
        &self,
        _handle: &ModelHandle,
        _acceleration: AccelerationMode,
    ) -> latexsnipper_foundation::Result<Box<dyn InferenceSession>> {
        Err(latexsnipper_foundation::SnipperError::Runtime(
            "provider unavailable".to_string(),
        ))
    }

    fn name(&self) -> &str {
        "unavailable-test"
    }

    fn is_available(&self) -> bool {
        false
    }
}

#[test]
fn health_reports_an_unavailable_provider_without_inference() {
    let models = tempfile::tempdir().unwrap();
    let engine = SnipperEngine::new(
        EngineConfig {
            models_dir: models.path().to_path_buf(),
            ..EngineConfig::default()
        },
        Box::new(UnavailableRuntime),
    );
    let session = RecognitionSession::from_engine(engine).unwrap();
    let health = session.health_check().unwrap();
    assert!(!health.ready);
    assert_eq!(health.runtime.runtimes.len(), 1);
    assert!(!health.runtime.runtimes[0].available);
}

struct PanickingProgress;

impl ProgressSink for PanickingProgress {
    fn report(&self, _event: ProgressEvent) {
        panic!("application progress callback failure");
    }
}

#[test]
fn progress_callback_failure_does_not_break_recognition() {
    let (_models, mut session) = session();
    let result = session.recognize_with_control(
        RecognitionRequest::from_image(blank_image()),
        RecognitionControl {
            progress: Some(Arc::new(PanickingProgress)),
            cancellation: None,
        },
    );
    assert!(result.is_ok());
}

struct ClearCountingRuntime {
    clears: Arc<AtomicUsize>,
}

impl RuntimeBackend for ClearCountingRuntime {
    fn create_session(
        &self,
        _handle: &ModelHandle,
        _acceleration: AccelerationMode,
    ) -> latexsnipper_foundation::Result<Box<dyn InferenceSession>> {
        Ok(Box::new(EmptySession))
    }

    fn clear_sessions(&self) {
        self.clears.fetch_add(1, Ordering::SeqCst);
    }

    fn name(&self) -> &str {
        "clear-counting"
    }

    fn is_available(&self) -> bool {
        true
    }
}

struct EmptySession;

impl InferenceSession for EmptySession {
    fn run(&self, _inputs: &[Tensor]) -> latexsnipper_foundation::Result<Vec<Tensor>> {
        Ok(Vec::new())
    }

    fn input_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn output_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn release(&mut self) {}
}

#[test]
fn close_releases_runtime_owned_session_caches_once() {
    let models = tempfile::tempdir().unwrap();
    let clears = Arc::new(AtomicUsize::new(0));
    let engine = SnipperEngine::new(
        EngineConfig {
            models_dir: models.path().to_path_buf(),
            ..EngineConfig::default()
        },
        Box::new(ClearCountingRuntime {
            clears: clears.clone(),
        }),
    );
    let mut session = RecognitionSession::from_engine(engine).unwrap();
    session.close();
    session.close();
    assert_eq!(clears.load(Ordering::SeqCst), 1);
}

#[test]
fn zero_timeout_is_structured_and_session_remains_usable() {
    let (_models, mut session) = session();
    let error = session
        .recognize(RecognitionRequest::from_image(blank_image()).with_options(
            latexsnipper_engine::application::RecognitionOptions {
                timeout: Some(std::time::Duration::ZERO),
                ..Default::default()
            },
        ))
        .unwrap_err();
    assert_eq!(error.code, ApplicationErrorCode::Timeout);
    assert!(session
        .recognize(RecognitionRequest::from_image(blank_image()))
        .is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn async_entrypoint_avoids_nested_runtime_panics() {
    let (_models, mut session) = session();
    let sync_error = session
        .recognize(RecognitionRequest::from_image(blank_image()))
        .unwrap_err();
    assert_eq!(sync_error.code, ApplicationErrorCode::InvalidInput);

    let result = session
        .recognize_async(RecognitionRequest::from_image(blank_image()))
        .await
        .unwrap();
    assert_eq!(result.document.pages.len(), 1);
}
