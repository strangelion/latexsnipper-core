use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_model::{RuntimeVariant, VariantStatus};
use latexsnipper_runtime::{
    AccelerationMode, DeviceKind, ExecutionProviderSpec, ManifestTensorSpec, ModelFiles,
    ModelHandle, ModelManifest, ModelTask, OnnxRuntimeBackend, RegistryRuntimeBackend, RunRequest,
    RunResponse, RuntimeArtifacts, RuntimeBackend, RuntimeCapabilities, RuntimeDevice,
    RuntimeFactory, RuntimeKind, RuntimeOptions, RuntimeProbe, RuntimeRegistry, RuntimeSession,
    SessionMetadata, SessionTensorSpec, TensorMap,
};
use latexsnipper_tensor::Tensor;

struct TestFactory {
    kind: RuntimeKind,
    available: bool,
    sessions_created: Arc<AtomicUsize>,
}

impl TestFactory {
    fn available(kind: RuntimeKind) -> (Self, Arc<AtomicUsize>) {
        let sessions_created = Arc::new(AtomicUsize::new(0));
        (
            Self {
                kind,
                available: true,
                sessions_created: sessions_created.clone(),
            },
            sessions_created,
        )
    }

    fn unavailable(kind: RuntimeKind) -> Self {
        Self {
            kind,
            available: false,
            sessions_created: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl RuntimeFactory for TestFactory {
    fn kind(&self) -> RuntimeKind {
        self.kind.clone()
    }

    fn probe(&self) -> RuntimeProbe {
        if self.available {
            RuntimeProbe {
                available: true,
                version: Some("test-1".to_owned()),
                devices: vec![RuntimeDevice {
                    name: "cpu".to_owned(),
                    kind: DeviceKind::Cpu,
                    memory_bytes: None,
                }],
                reason_unavailable: None,
                capabilities: RuntimeCapabilities::default(),
            }
        } else {
            RuntimeProbe::unavailable("test runtime intentionally unavailable")
        }
    }

    fn create_session(
        &self,
        _artifacts: &RuntimeArtifacts,
        _options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        self.sessions_created.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(EchoSession {
            metadata: SessionMetadata {
                runtime: self.kind.clone(),
                model_id: Some("echo".to_owned()),
                methods: Vec::new(),
                inputs: vec![SessionTensorSpec {
                    name: "x".to_owned(),
                    shape: vec![Some(1)],
                    dtype: "f32".to_owned(),
                }],
                outputs: vec![SessionTensorSpec {
                    name: "y".to_owned(),
                    shape: vec![Some(1)],
                    dtype: "f32".to_owned(),
                }],
            },
        }))
    }
}

struct EchoSession {
    metadata: SessionMetadata,
}

impl RuntimeSession for EchoSession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        let input = request
            .inputs
            .get("x")
            .ok_or_else(|| SnipperError::Inference("missing x".to_owned()))?;
        Ok(RunResponse {
            outputs: TensorMap::from([(
                "y".to_owned(),
                Tensor::float32(
                    "y",
                    input.shape().to_vec(),
                    input.as_f32_slice().unwrap().to_vec(),
                ),
            )]),
        })
    }
}

fn base_manifest() -> ModelManifest {
    ModelManifest {
        id: "test-model".to_owned(),
        task: ModelTask::FormulaRecognition,
        version: "1.0".to_owned(),
        adapter: "test-adapter".to_owned(),
        input: ManifestTensorSpec {
            name: "x".to_owned(),
            shape: vec![1],
            dtype: "float32".to_owned(),
        },
        output: vec![ManifestTensorSpec {
            name: "y".to_owned(),
            shape: vec![1],
            dtype: "float32".to_owned(),
        }],
        files: ModelFiles::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: Vec::new(),
    }
}

fn existing_artifact() -> (&'static Path, &'static str) {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../wasm/tests/fixtures/tiny-text-rec.onnx");
    let path = Box::leak(path.into_boxed_path());
    (path.parent().unwrap(), "tiny-text-rec.onnx")
}

fn variant(id: &str, runtime: &str, priority: i32, artifact: &str) -> RuntimeVariant {
    RuntimeVariant {
        id: id.to_owned(),
        runtime: runtime.to_owned(),
        status: VariantStatus::Stable,
        priority,
        artifacts: BTreeMap::from([("model".to_owned(), artifact.to_owned())]),
        options: None,
        platforms: Vec::new(),
        capabilities: Vec::new(),
        fallbacks: Vec::new(),
    }
}

#[test]
fn registry_register_probe_and_execute_named_tensors() {
    let (factory, sessions_created) = TestFactory::available(RuntimeKind::OnnxRuntime);
    let mut registry = RuntimeRegistry::new();
    registry.register(factory).unwrap();
    assert_eq!(
        registry.available_runtimes(),
        vec![RuntimeKind::OnnxRuntime]
    );

    let (model_dir, artifact) = existing_artifact();
    let mut manifest = base_manifest();
    manifest.runtime_variants = vec![variant("onnx", "onnx-runtime", 10, artifact)];
    let resolved = manifest.resolve_runtime(&registry, model_dir).unwrap();
    let session = registry.create_resolved_session(&resolved).unwrap();
    let response = session
        .run(RunRequest::new(TensorMap::from([(
            "x".to_owned(),
            Tensor::float32("x", vec![1], vec![42.0]),
        )])))
        .unwrap();
    assert_eq!(
        response.outputs["y"].as_f32_slice(),
        Some([42.0].as_slice())
    );
    assert_eq!(sessions_created.load(Ordering::SeqCst), 1);
}

#[test]
fn legacy_onnx_manifest_derives_an_implicit_variant() {
    let (factory, _) = TestFactory::available(RuntimeKind::OnnxRuntime);
    let registry = RuntimeRegistry::with_factory(factory);
    let (model_dir, artifact) = existing_artifact();
    let mut manifest = base_manifest();
    manifest.files.primary = Some(artifact.to_owned());

    let resolved = manifest.resolve_runtime(&registry, model_dir).unwrap();
    assert_eq!(resolved.variant_id, "onnx-default");
    assert_eq!(resolved.runtime, RuntimeKind::OnnxRuntime);
    assert_eq!(resolved.artifacts.files["model"], model_dir.join(artifact));
}

#[test]
fn paddle_only_manifest_parses_without_legacy_files() {
    let parsed: ModelManifest = toml::from_str(
        r#"
id = "pp-formulanet-s"
task = "FormulaRecognition"
version = "1"
adapter = "pp-formulanet-v1"

[input]
name = "image"
shape = [1, 1, 384, 384]
dtype = "float32"

[[output]]
name = "tokens"
shape = [1, -1]
dtype = "int64"

[[runtimeVariants]]
id = "paddle-native"
runtime = "paddle-inference"
status = "stable"
priority = 100

[runtimeVariants.artifacts]
model = "inference.json"
params = "inference.pdiparams"
"#,
    )
    .unwrap();
    assert!(parsed.files.primary.is_none());
    assert_eq!(parsed.runtime_variants.len(), 1);
    assert_eq!(parsed.runtime_variants[0].runtime, "paddle-inference");
}

#[test]
fn unavailable_preferred_runtime_uses_only_explicit_fallback() {
    let (onnx, _) = TestFactory::available(RuntimeKind::OnnxRuntime);
    let mut registry = RuntimeRegistry::new();
    registry
        .register(TestFactory::unavailable(RuntimeKind::PaddleInference))
        .unwrap();
    registry.register(onnx).unwrap();
    let (model_dir, artifact) = existing_artifact();

    let mut paddle = variant("paddle", "paddle-inference", 100, "missing.pdmodel");
    paddle.fallbacks = vec!["onnx".to_owned()];
    let onnx = variant("onnx", "onnx-runtime", 50, artifact);
    let mut manifest = base_manifest();
    manifest.runtime_variants = vec![paddle, onnx];

    let resolved = manifest.resolve_runtime(&registry, model_dir).unwrap();
    assert_eq!(resolved.variant_id, "onnx");
    assert_eq!(resolved.fallback_from.as_deref(), Some("paddle"));
}

#[test]
fn unavailable_runtime_never_falls_back_to_an_unlisted_variant() {
    let (onnx, _) = TestFactory::available(RuntimeKind::OnnxRuntime);
    let mut registry = RuntimeRegistry::new();
    registry
        .register(TestFactory::unavailable(RuntimeKind::PaddleInference))
        .unwrap();
    registry.register(onnx).unwrap();
    let (model_dir, artifact) = existing_artifact();
    let mut manifest = base_manifest();
    manifest.runtime_variants = vec![
        variant("paddle", "paddle-inference", 100, "missing.pdmodel"),
        variant("onnx", "onnx-runtime", 50, artifact),
    ];

    let error = manifest.resolve_runtime(&registry, model_dir).unwrap_err();
    assert!(error.to_string().contains("no runnable runtime variant"));
    assert!(!error.to_string().contains("onnx ("));
}

#[test]
fn deprecated_variant_is_not_a_root_candidate() {
    let (onnx, _) = TestFactory::available(RuntimeKind::OnnxRuntime);
    let registry = RuntimeRegistry::with_factory(onnx);
    let (model_dir, artifact) = existing_artifact();
    let mut deprecated = variant("old", "onnx-runtime", 100, artifact);
    deprecated.status = VariantStatus::Deprecated;
    let current = variant("current", "onnx-runtime", 10, artifact);
    let mut manifest = base_manifest();
    manifest.runtime_variants = vec![deprecated, current];

    assert_eq!(
        manifest
            .resolve_runtime(&registry, model_dir)
            .unwrap()
            .variant_id,
        "current"
    );
}

#[test]
fn unknown_runtime_reports_unregistered_instead_of_becoming_onnx() {
    let registry = RuntimeRegistry::new();
    let (model_dir, artifact) = existing_artifact();
    let mut manifest = base_manifest();
    manifest.runtime_variants = vec![variant("vendor", "custom:vendor-npu", 1, artifact)];
    let error = manifest.resolve_runtime(&registry, model_dir).unwrap_err();
    assert!(error.to_string().contains("runtime is not registered"));
}

#[test]
fn legacy_backend_compatibility_is_a_registry_adapter_not_a_second_path() {
    let (factory, sessions_created) = TestFactory::available(RuntimeKind::OnnxRuntime);
    let registry = Arc::new(RuntimeRegistry::with_factory(factory));
    let backend = RegistryRuntimeBackend::new(registry, RuntimeKind::OnnxRuntime);
    let (model_dir, artifact) = existing_artifact();
    let handle = ModelHandle::with_path("echo", model_dir.join(artifact));
    let session = backend
        .create_session(&handle, AccelerationMode::Cpu)
        .unwrap();
    let outputs = session
        .run(&[Tensor::float32("x", vec![1], vec![7.0])])
        .unwrap();
    assert_eq!(outputs[0].as_f32_slice(), Some([7.0].as_slice()));
    assert_eq!(sessions_created.load(Ordering::SeqCst), 1);
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
#[test]
fn real_onnx_registry_inference_matches_legacy_execution() {
    use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;

    let (model_dir, artifact) = existing_artifact();
    let model_path = model_dir.join(artifact);
    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.25; 3 * 48 * 320]);

    let legacy_backend = OnnxRuntimeBackend::new(model_dir.to_path_buf()).unwrap();
    let legacy = legacy_backend
        .create_session(
            &ModelHandle::with_path("tiny-text-rec", model_path.clone()),
            AccelerationMode::Cpu,
        )
        .unwrap();
    let legacy_outputs = legacy.run(std::slice::from_ref(&input)).unwrap();

    let registry = RuntimeRegistry::with_factory(OnnxRuntimeFactory::new(model_dir.to_path_buf()));
    let resolved = latexsnipper_runtime::RuntimeResolver::new(&registry)
        .resolve(
            "tiny-text-rec",
            &[variant("onnx", "onnx-runtime", 1, artifact)],
            model_dir,
            None,
        )
        .unwrap();
    let session = registry.create_resolved_session(&resolved).unwrap();
    let registry_outputs = session
        .run(RunRequest::new(TensorMap::from([("x".to_owned(), input)])))
        .unwrap();

    assert_eq!(legacy_outputs.len(), registry_outputs.outputs.len());
    for legacy in legacy_outputs {
        let current = &registry_outputs.outputs[legacy.name()];
        assert_eq!(legacy.shape(), current.shape());
        assert_eq!(legacy.as_f32_slice(), current.as_f32_slice());
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
#[test]
fn onnx_provider_chain_runs_with_only_the_declared_cpu_fallback() {
    use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;

    let (model_dir, artifact) = existing_artifact();
    let factory = OnnxRuntimeFactory::new(model_dir.to_path_buf());
    let artifacts = RuntimeArtifacts::new(RuntimeKind::OnnxRuntime)
        .with_file("model", model_dir.join(artifact));
    let options = RuntimeOptions {
        providers: vec![
            ExecutionProviderSpec::new("tensorrt"),
            ExecutionProviderSpec::new("cuda"),
            ExecutionProviderSpec::cpu(),
        ],
        ..RuntimeOptions::default()
    };
    let session = factory.create_session(&artifacts, &options).unwrap();
    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.25; 3 * 48 * 320]);
    let response = session
        .run(RunRequest::new(TensorMap::from([("x".to_owned(), input)])))
        .unwrap();
    assert!(!response.outputs.is_empty());
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
#[test]
fn onnx_provider_chain_does_not_invent_cpu_fallback() {
    use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;

    let (model_dir, artifact) = existing_artifact();
    let factory = OnnxRuntimeFactory::new(model_dir.to_path_buf());
    let artifacts = RuntimeArtifacts::new(RuntimeKind::OnnxRuntime)
        .with_file("model", model_dir.join(artifact));
    let options = RuntimeOptions {
        providers: vec![ExecutionProviderSpec::new("qnn")],
        ..RuntimeOptions::default()
    };
    let error = match factory.create_session(&artifacts, &options) {
        Ok(_) => panic!("QNN unexpectedly ran without a registered provider or CPU fallback"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("CPU fallback was not declared"));
}
