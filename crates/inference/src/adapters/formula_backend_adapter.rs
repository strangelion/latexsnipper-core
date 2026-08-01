//! Generic formula backend adapter — wraps OnnxFormulaBackend as a ModelPackage.
//!
//! Any encoder-decoder ONNX formula model can be plugged in by just
//! providing ONNX files + vocab + config.json in the model directory.

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    InferenceContext, ModelDescriptor, ModelExecutionContext, ModelExecutor, ModelId, ModelInput,
    ModelOutput, ModelPackage, ModelTask, RuntimeBackend, TensorDtype, TensorSpec,
};

use crate::formula_backend::{BackendConfig, FormulaBackend, OnnxFormulaBackend};

/// Generic ONNX formula backend package.
///
/// Works with any encoder-decoder ONNX pair. The model directory should contain:
/// - `encoder.onnx` (or `pp-formulanet.onnx`, etc.)
/// - `decoder.onnx` (or `decoder_model.onnx`, etc.)
/// - `vocab.txt` or `tokenizer.json`
/// - `config.json`
pub struct FormulaBackendPackage {
    descriptor: ModelDescriptor,
    model_dir: std::path::PathBuf,
    config: BackendConfig,
}

impl FormulaBackendPackage {
    /// Create from model config and directory.
    pub fn from_config(
        config: &ModelConfig,
        model_id: ModelId,
        model_dir: std::path::PathBuf,
    ) -> Self {
        let backend_config = BackendConfig::from_config(config);

        let input_size = config
            .input
            .as_ref()
            .and_then(|i| i.shape.get(2))
            .copied()
            .unwrap_or(512) as usize;

        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::FormulaRecognition,
            version: config
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            input_spec: TensorSpec {
                name: "image".into(),
                shape: vec![1, 3, input_size, input_size],
                dtype: TensorDtype::Float32,
            },
            output_spec: vec![TensorSpec {
                name: "logits".into(),
                shape: vec![1, 0, 0],
                dtype: TensorDtype::Float32,
            }],
            artifact_paths: vec![],
        };

        Self {
            descriptor,
            model_dir,
            config: backend_config,
        }
    }
}

impl ModelPackage for FormulaBackendPackage {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>> {
        let diagnostics = runtime.runtime_diagnostics();
        let mut executor = FormulaBackendExecutor {
            descriptor: self.descriptor.clone(),
            model_dir: self.model_dir.clone(),
            config: self.config.clone(),
            runtime,
            backend: None,
            runtime_id: diagnostics.runtime,
            provider: diagnostics.selected_provider,
        };
        executor.ensure_backend()?;
        Ok(Box::new(executor))
    }

    fn create_executor_with_context(
        &self,
        ctx: &ModelExecutionContext,
    ) -> Result<Box<dyn ModelExecutor>> {
        let provider = ctx
            .resolved_runtime
            .options
            .providers
            .first()
            .map(|provider| provider.name.clone())
            .unwrap_or_else(|| "runtime-default".to_owned());
        let mut executor = FormulaBackendExecutor {
            descriptor: self.descriptor.clone(),
            model_dir: self.model_dir.clone(),
            config: self.config.clone(),
            runtime: ctx.backend_compat(),
            backend: None,
            runtime_id: ctx.resolved_runtime.runtime.to_string(),
            provider,
        };
        executor.ensure_backend()?;
        Ok(Box::new(executor))
    }
}

/// Executor that lazily loads the ONNX backend on first call.
struct FormulaBackendExecutor {
    descriptor: ModelDescriptor,
    model_dir: std::path::PathBuf,
    #[allow(dead_code)]
    config: BackendConfig,
    runtime: Arc<dyn RuntimeBackend>,
    backend: Option<OnnxFormulaBackend>,
    runtime_id: String,
    provider: String,
}

impl FormulaBackendExecutor {
    fn ensure_backend(&mut self) -> Result<&OnnxFormulaBackend> {
        if self.backend.is_none() {
            let backend = OnnxFormulaBackend::load(&self.model_dir, &*self.runtime)?;
            self.provider = self.runtime.selected_provider();
            self.backend = Some(backend);
        }
        Ok(self.backend.as_ref().unwrap())
    }
}

impl ModelExecutor for FormulaBackendExecutor {
    fn run(&mut self, input: ModelInput, _ctx: &mut InferenceContext) -> Result<ModelOutput> {
        let backend = self.ensure_backend()?;

        // Reconstruct SnipperImage from ModelInput
        let shape = &input.shape;
        if shape.len() != 3 {
            return Err(SnipperError::Inference(format!(
                "Expected 3D shape [H, W, 3], got {:?}",
                shape
            )));
        }
        let height = shape[0] as u32;
        let width = shape[1] as u32;
        let pixels: Vec<u8> = input.data.to_vec();

        let image = latexsnipper_image::SnipperImage::new(
            width,
            height,
            latexsnipper_image::color::PixelFormat::Rgb,
            pixels,
        );

        let result = backend.recognize(&image)?.ensure_runtime_provenance(
            self.descriptor.id.composite_key(),
            self.descriptor.version.clone(),
            self.runtime_id.clone(),
            self.provider.clone(),
        );

        Ok(ModelOutput::Formula(vec![
            latexsnipper_runtime::FormulaResult {
                latex: result.text,
                confidence: result.confidence,
                provenance: result.provenance,
                evidence: result.postprocess,
            },
        ]))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}

use std::sync::Arc;

/// Register the generic ONNX formula backend adapter.
pub fn register(registry: &mut latexsnipper_runtime::ModelRegistry) {
    registry.register_adapter("onnx-formula-v1", |manifest, model_dir| {
        let model_id = ModelId::from_composite_key(&manifest.id);
        let config_exists = model_dir.join("config.json").exists();

        let config = if config_exists {
            ModelConfig::load(model_dir).ok()
        } else {
            Some(manifest_to_config(manifest))
        };

        let config = config.unwrap_or_else(ModelConfig::minimal);

        let package =
            FormulaBackendPackage::from_config(&config, model_id, model_dir.to_path_buf());
        Ok(Box::new(package))
    });
}

fn manifest_to_config(manifest: &latexsnipper_runtime::ModelManifest) -> ModelConfig {
    let input = Some(latexsnipper_model::InputConfig {
        name: manifest.input.name.clone(),
        shape: manifest.input.shape.clone(),
        dtype: manifest.input.dtype.clone(),
        range: None,
    });

    let output = manifest
        .output
        .first()
        .map(|o| latexsnipper_model::OutputConfig {
            name: o.name.clone(),
            shape: o.shape.clone(),
            description: None,
        });

    let preprocessing =
        manifest
            .preprocessing
            .as_ref()
            .map(|p| latexsnipper_model::PreprocessConfig {
                resize: p.resize.as_ref().map(|r| latexsnipper_model::ResizeConfig {
                    width: r.width,
                    height: r.height,
                    keep_ratio: r.keep_ratio,
                    pad_value: None,
                }),
                normalization: Some(latexsnipper_model::NormalizationConfig {
                    mean: p.mean.clone(),
                    std: p.std.clone(),
                }),
                color_format: p.color_format.clone(),
                divisible_by: None,
                pad_value: None,
            });

    ModelConfig {
        model_type: "onnx_formula".into(),
        model_family: Some(format!("{} v{}", manifest.adapter, manifest.version)),
        license: None,
        task_type: Some("formula_recognition".into()),
        num_classes: None,
        dynamic_shapes: None,
        input,
        output,
        encoder: None,
        decoder: None,
        preprocessing,
        postprocessing: None,
        decoding: None,
        quantization: None,
        outputs: None,
        extra: None,
        pipeline: None,
        runtime_variants: manifest.runtime_variants.clone(),
    }
}
