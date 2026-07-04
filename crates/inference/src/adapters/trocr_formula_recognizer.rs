use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    AccelerationMode, InferenceContext, InferenceSession, ModelDescriptor, ModelExecutor, ModelId,
    ModelInput, ModelOutput, ModelPackage, ModelTask, RuntimeBackend, TensorDtype, TensorSpec,
};
use std::path::PathBuf;
use std::sync::Arc;

/// TrOCR formula recognition model package.
pub struct TrOcrFormulaPackage {
    descriptor: ModelDescriptor,
    encoder_path: Option<PathBuf>,
    decoder_path: Option<PathBuf>,
    tokenizer_path: Option<PathBuf>,
}

impl TrOcrFormulaPackage {
    /// Create from a model config.
    pub fn from_config(config: &ModelConfig, model_id: ModelId) -> Self {
        let input_size = config
            .encoder
            .as_ref()
            .and_then(|e| e.input.shape.get(2))
            .copied()
            .unwrap_or(384) as usize;

        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::FormulaRecognition,
            version: config
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            input_spec: TensorSpec {
                name: "pixel_values".into(),
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
            encoder_path: None,
            decoder_path: None,
            tokenizer_path: None,
        }
    }

    /// Set model paths for encoder, decoder, and tokenizer.
    pub fn with_paths(mut self, encoder: PathBuf, decoder: PathBuf, tokenizer: PathBuf) -> Self {
        self.encoder_path = Some(encoder);
        self.decoder_path = Some(decoder);
        self.tokenizer_path = Some(tokenizer);
        self
    }
}

impl ModelPackage for TrOcrFormulaPackage {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>> {
        Ok(Box::new(TrOcrFormulaExecutor {
            descriptor: self.descriptor.clone(),
            runtime,
            encoder_path: self.encoder_path.clone(),
            decoder_path: self.decoder_path.clone(),
            tokenizer_path: self.tokenizer_path.clone(),
            encoder_session: None,
            decoder_session: None,
        }))
    }
}

/// Executor for TrOCR formula recognition.
///
/// Input: `ModelInput` with RGB image bytes (name="image", shape=[H, W, 3])
/// Output: `ModelOutput::Formula` with LaTeX strings
struct TrOcrFormulaExecutor {
    descriptor: ModelDescriptor,
    runtime: Arc<dyn RuntimeBackend>,
    encoder_path: Option<PathBuf>,
    decoder_path: Option<PathBuf>,
    tokenizer_path: Option<PathBuf>,
    encoder_session: Option<Arc<Box<dyn InferenceSession>>>,
    decoder_session: Option<Arc<Box<dyn InferenceSession>>>,
}

/// Type alias for session references.
type SessionRef<'a> = &'a Arc<Box<dyn InferenceSession>>;

impl TrOcrFormulaExecutor {
    /// Ensure sessions are loaded, creating from paths if needed.
    #[allow(clippy::unnecessary_unwrap)]
    fn ensure_sessions(&mut self) -> Result<(SessionRef<'_>, SessionRef<'_>, &PathBuf)> {
        if self.encoder_session.is_some()
            && self.decoder_session.is_some()
            && self.tokenizer_path.is_some()
        {
            return Ok((
                self.encoder_session.as_ref().unwrap(),
                self.decoder_session.as_ref().unwrap(),
                self.tokenizer_path.as_ref().unwrap(),
            ));
        }

        let encoder_path = self.encoder_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No encoder path configured for TrOcrFormula".into())
        })?;
        let decoder_path = self.decoder_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No decoder path configured for TrOcrFormula".into())
        })?;
        // Validate tokenizer path exists
        if self.tokenizer_path.is_none() {
            return Err(SnipperError::Inference(
                "No tokenizer path configured for TrOcrFormula".into(),
            ));
        }

        let enc_handle = latexsnipper_runtime::ModelHandle::with_path(
            format!("{}/encoder", self.descriptor.id.composite_key()),
            encoder_path.to_path_buf(),
        );
        let dec_handle = latexsnipper_runtime::ModelHandle::with_path(
            format!("{}/decoder", self.descriptor.id.composite_key()),
            decoder_path.to_path_buf(),
        );

        let enc_session = self
            .runtime
            .create_session(&enc_handle, AccelerationMode::Cpu)?;
        let dec_session = self
            .runtime
            .create_session(&dec_handle, AccelerationMode::Cpu)?;

        self.encoder_session = Some(Arc::new(enc_session));
        self.decoder_session = Some(Arc::new(dec_session));

        Ok((
            self.encoder_session.as_ref().unwrap(),
            self.decoder_session.as_ref().unwrap(),
            self.tokenizer_path.as_ref().unwrap(),
        ))
    }
}

impl ModelExecutor for TrOcrFormulaExecutor {
    fn run(&mut self, input: ModelInput, _ctx: &mut InferenceContext) -> Result<ModelOutput> {
        let (encoder, decoder, tokenizer_path) = self.ensure_sessions()?;

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

        let image = SnipperImage::new(
            width,
            height,
            latexsnipper_image::color::PixelFormat::Rgb,
            pixels,
        );

        // Run recognition using the existing inference function
        let params = crate::formula_recognizer::RecognitionParams::default();
        let result = crate::formula_recognizer::recognize_formula(
            &image,
            &**encoder,
            &**decoder,
            tokenizer_path,
            &params,
        )?;

        // Convert to ModelOutput
        Ok(ModelOutput::Formula(vec![
            latexsnipper_runtime::FormulaResult {
                latex: result.text,
                confidence: result.confidence,
            },
        ]))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}
