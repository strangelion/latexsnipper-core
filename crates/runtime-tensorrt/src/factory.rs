use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    DeviceKind, RuntimeArtifacts, RuntimeCapabilities, RuntimeDevice, RuntimeFactory, RuntimeKind,
    RuntimeOptions, RuntimeProbe, RuntimeSession,
};

use crate::cache::{cache_key, read_artifact, EngineCache};
use crate::error::tensorrt_error;
use crate::ffi::{TensorRtApi, TensorRtProgram};
use crate::flavor::TensorRtFlavor;
use crate::loader::TensorRtLibraryLocator;
use crate::options::{TensorRtOptions, TensorRtPrecision};
use crate::session::TensorRtSession;

static ENGINE_BUILD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone)]
struct NativeFactory {
    flavor: TensorRtFlavor,
    library_path: Option<PathBuf>,
}

impl NativeFactory {
    fn new(flavor: TensorRtFlavor, library_path: Option<PathBuf>) -> Self {
        Self {
            flavor,
            library_path,
        }
    }

    fn locator(&self, options: Option<&TensorRtOptions>) -> TensorRtLibraryLocator {
        TensorRtLibraryLocator::new(
            options
                .and_then(|options| options.library_path.clone())
                .or_else(|| self.library_path.clone()),
            self.flavor,
        )
    }

    fn probe(&self) -> RuntimeProbe {
        match self
            .locator(None)
            .load()
            .and_then(|api| api.device_info(0).map(|device| (api, device)))
        {
            Ok((api, (fingerprint, memory_bytes))) => RuntimeProbe {
                available: true,
                version: api.version(),
                devices: vec![RuntimeDevice {
                    name: fingerprint,
                    kind: DeviceKind::Gpu,
                    memory_bytes: Some(memory_bytes),
                }],
                reason_unavailable: None,
                capabilities: RuntimeCapabilities {
                    tensor_dtypes: BTreeSet::from([
                        "f32".to_owned(),
                        "f16".to_owned(),
                        "i64".to_owned(),
                        "i32".to_owned(),
                        "u8".to_owned(),
                        "bool".to_owned(),
                    ]),
                    execution_providers: BTreeSet::from(["cuda".to_owned()]),
                    methods: BTreeSet::new(),
                    features: self.features(),
                },
            },
            Err(error) => RuntimeProbe::unavailable(error.to_string()),
        }
    }

    fn features(&self) -> BTreeSet<String> {
        let mut features = BTreeSet::from([
            "versioned-c-bridge".to_owned(),
            "onnx-engine-build".to_owned(),
            "serialized-engine".to_owned(),
            "dynamic-shapes".to_owned(),
            "engine-cache-v1".to_owned(),
            "cpu-copy".to_owned(),
        ]);
        if self.flavor == TensorRtFlavor::Rtx {
            features.extend([
                "rtx-aot-jit".to_owned(),
                "strongly-typed-model".to_owned(),
                "on-device-aot".to_owned(),
            ]);
        }
        features
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        let expected_runtime = self.flavor.runtime_kind();
        if artifacts.runtime != expected_runtime {
            return Err(tensorrt_error(format!(
                "{} factory received '{}' artifacts",
                self.flavor.display_name(),
                artifacts.runtime
            )));
        }
        let options = TensorRtOptions::from_runtime(options)?;
        if self.flavor == TensorRtFlavor::Rtx && options.precision != TensorRtPrecision::Fp32 {
            return Err(tensorrt_error(
                "TensorRT-RTX 1.5 uses strongly typed ONNX models; encode FP16 or quantization in the model and omit the TensorRT 10 precision option",
            ));
        }
        let engine_path = engine_artifact(artifacts, self.flavor)?;
        let onnx_path = if engine_path.is_none() {
            Some(onnx_artifact(artifacts, self.flavor)?)
        } else {
            None
        };
        let api = self.locator(Some(&options)).load()?;
        let (model_id, program) = if let Some(engine_path) = engine_path {
            ensure_regular_file(engine_path, "engine")?;
            let engine = read_artifact(engine_path, self.flavor.display_name())?;
            let program = TensorRtProgram::load(api, &engine, options.device_id)?;
            (engine_path.to_string_lossy().into_owned(), program)
        } else {
            let onnx_path = onnx_path.expect("ONNX artifact was resolved before loading runtime");
            ensure_regular_file(onnx_path, "ONNX source")?;
            let program = build_or_load_cached(&api, onnx_path, &options, self.flavor)?;
            (onnx_path.to_string_lossy().into_owned(), program)
        };
        Ok(Box::new(TensorRtSession::new(
            expected_runtime,
            Some(model_id),
            program,
        )?))
    }
}

#[derive(Debug, Clone)]
pub struct TensorRtFactory {
    inner: NativeFactory,
}

impl Default for TensorRtFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl TensorRtFactory {
    pub fn new() -> Self {
        Self {
            inner: NativeFactory::new(TensorRtFlavor::Standard, None),
        }
    }

    pub fn with_library_path(path: impl Into<PathBuf>) -> Self {
        Self {
            inner: NativeFactory::new(TensorRtFlavor::Standard, Some(path.into())),
        }
    }
}

impl RuntimeFactory for TensorRtFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::TensorRt
    }

    fn probe(&self) -> RuntimeProbe {
        self.inner.probe()
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        self.inner.create_session(artifacts, options)
    }
}

#[derive(Debug, Clone)]
pub struct TensorRtRtxFactory {
    inner: NativeFactory,
}

impl Default for TensorRtRtxFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl TensorRtRtxFactory {
    pub fn new() -> Self {
        Self {
            inner: NativeFactory::new(TensorRtFlavor::Rtx, None),
        }
    }

    pub fn with_library_path(path: impl Into<PathBuf>) -> Self {
        Self {
            inner: NativeFactory::new(TensorRtFlavor::Rtx, Some(path.into())),
        }
    }
}

impl RuntimeFactory for TensorRtRtxFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::TensorRtRtx
    }

    fn probe(&self) -> RuntimeProbe {
        self.inner.probe()
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        self.inner.create_session(artifacts, options)
    }
}

fn build_or_load_cached(
    api: &std::sync::Arc<TensorRtApi>,
    onnx_path: &Path,
    options: &TensorRtOptions,
    flavor: TensorRtFlavor,
) -> Result<TensorRtProgram> {
    let onnx = read_artifact(onnx_path, "ONNX source")?;
    let runtime_version = api.version().unwrap_or_else(|| "unknown".to_owned());
    let (device_fingerprint, _) = api.device_info(options.device_id)?;
    if !options.cache {
        let engine = api.build_engine(onnx_path, options)?;
        return TensorRtProgram::load(std::sync::Arc::clone(api), &engine, options.device_id);
    }
    let key = cache_key(
        &onnx,
        flavor,
        &runtime_version,
        &device_fingerprint,
        options,
    )?;
    let cache = EngineCache::new(options.cache_dir.clone());
    if let Some(engine) = cache.load(&key)? {
        match TensorRtProgram::load(std::sync::Arc::clone(api), &engine, options.device_id) {
            Ok(program) => return Ok(program),
            Err(error) => {
                log::warn!(
                    "Invalidating {} engine cache key {key} after load failure: {error}",
                    flavor.display_name()
                );
                cache.invalidate(&key)?;
            }
        }
    }

    let lock = ENGINE_BUILD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| tensorrt_error("engine build lock was poisoned"))?;
    if let Some(engine) = cache.load(&key)? {
        if let Ok(program) =
            TensorRtProgram::load(std::sync::Arc::clone(api), &engine, options.device_id)
        {
            return Ok(program);
        }
        cache.invalidate(&key)?;
    }
    let engine = api.build_engine(onnx_path, options)?;
    cache.store(&key, &engine)?;
    TensorRtProgram::load(std::sync::Arc::clone(api), &engine, options.device_id)
}

fn engine_artifact(
    artifacts: &RuntimeArtifacts,
    flavor: TensorRtFlavor,
) -> Result<Option<&PathBuf>> {
    if let Some(path) = artifacts.files.get("engine") {
        if !is_engine_extension(path, flavor) {
            let expected = match flavor {
                TensorRtFlavor::Standard => ".engine or .plan",
                TensorRtFlavor::Rtx => ".rtxplan",
            };
            return Err(tensorrt_error(format!(
                "{} engine artifact must use {expected}: {}",
                flavor.display_name(),
                path.display()
            )));
        }
        return Ok(Some(path));
    }
    Ok(artifacts
        .files
        .values()
        .find(|path| is_engine_extension(path, flavor)))
}

fn is_engine_extension(path: &Path, flavor: TensorRtFlavor) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| match flavor {
            TensorRtFlavor::Standard => {
                extension.eq_ignore_ascii_case("engine") || extension.eq_ignore_ascii_case("plan")
            }
            TensorRtFlavor::Rtx => extension.eq_ignore_ascii_case("rtxplan"),
        })
}

fn onnx_artifact(artifacts: &RuntimeArtifacts, flavor: TensorRtFlavor) -> Result<&PathBuf> {
    artifacts
        .files
        .get("source")
        .or_else(|| artifacts.files.get("model"))
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("onnx"))
        })
        .ok_or_else(|| {
            let extension = match flavor {
                TensorRtFlavor::Standard => ".engine/.plan",
                TensorRtFlavor::Rtx => ".rtxplan",
            };
            tensorrt_error(format!(
                "artifacts must declare either an 'engine' ({extension}) or an ONNX 'source'/'model'"
            ))
        })
}

fn ensure_regular_file(path: &Path, kind: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(tensorrt_error(format!(
            "{kind} artifact is not a readable file: {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_runtime_has_actionable_probe_reason() {
        let impossible = std::env::temp_dir().join("latexsnipper-missing-tensorrt-runtime.dll");
        let probe = TensorRtFactory::with_library_path(impossible).probe();
        assert!(!probe.available);
        let reason = probe.reason_unavailable.unwrap();
        assert!(reason.contains("LATEXSNIPPER_TENSORRT_HOME"));
        assert!(reason.contains("not installed"));
    }

    #[test]
    fn absent_rtx_runtime_has_an_independent_probe_reason() {
        let impossible = std::env::temp_dir().join("latexsnipper-missing-tensorrt-rtx-runtime.dll");
        let probe = TensorRtRtxFactory::with_library_path(impossible).probe();
        assert!(!probe.available);
        let reason = probe.reason_unavailable.unwrap();
        assert!(reason.contains("LATEXSNIPPER_TENSORRT_RTX_HOME"));
        assert!(reason.contains("TensorRT-RTX"));
    }

    #[test]
    fn rejects_missing_artifact_before_native_build() {
        let artifacts = RuntimeArtifacts::new(RuntimeKind::TensorRt);
        let error = TensorRtFactory::new()
            .create_session(&artifacts, &RuntimeOptions::default())
            .err()
            .expect("missing artifact must fail");
        assert!(error.to_string().contains("must declare either"));
    }

    #[test]
    fn rtx_rejects_tensor_rt_10_precision_flags_before_loading_native_code() {
        let artifacts = RuntimeArtifacts::new(RuntimeKind::TensorRtRtx)
            .with_file("source", PathBuf::from("model.onnx"));
        let mut options = RuntimeOptions::default();
        options.extra.insert(
            "precision".to_owned(),
            serde_json::Value::String("fp16".to_owned()),
        );
        let error = TensorRtRtxFactory::new()
            .create_session(&artifacts, &options)
            .err()
            .expect("RTX must reject weak-typing flags");
        assert!(error.to_string().contains("strongly typed ONNX"));
    }

    #[test]
    fn engine_formats_cannot_cross_runtime_boundaries() {
        let rtx_artifacts = RuntimeArtifacts::new(RuntimeKind::TensorRtRtx)
            .with_file("engine", PathBuf::from("model.plan"));
        let rtx_error = TensorRtRtxFactory::new()
            .create_session(&rtx_artifacts, &RuntimeOptions::default())
            .err()
            .expect("RTX must reject a traditional plan");
        assert!(rtx_error.to_string().contains("must use .rtxplan"));

        let standard_artifacts = RuntimeArtifacts::new(RuntimeKind::TensorRt)
            .with_file("engine", PathBuf::from("model.rtxplan"));
        let standard_error = TensorRtFactory::new()
            .create_session(&standard_artifacts, &RuntimeOptions::default())
            .err()
            .expect("traditional TensorRT must reject an RTX plan");
        assert!(standard_error.to_string().contains(".engine or .plan"));
    }
}
