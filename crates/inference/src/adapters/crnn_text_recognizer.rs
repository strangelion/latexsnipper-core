use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    AccelerationMode, InferenceContext, InferenceSession, ModelDescriptor, ModelExecutor, ModelId,
    ModelInput, ModelOutput, ModelPackage, ModelTask, RuntimeBackend, TensorDtype, TensorSpec,
};
use std::path::PathBuf;
use std::sync::Arc;

/// CRNN CTC text recognition model package.
pub struct CrnnTextRecognizerPackage {
    descriptor: ModelDescriptor,
    model_path: Option<PathBuf>,
    keys_path: Option<PathBuf>,
}

impl CrnnTextRecognizerPackage {
    /// Create from a model config.
    pub fn from_config(config: &ModelConfig, model_id: ModelId) -> Self {
        let input_shape = config
            .input
            .as_ref()
            .map(|i| i.shape.iter().map(|s| *s as usize).collect())
            .unwrap_or_else(|| vec![1, 3, 48, 3200]);

        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::TextRecognition,
            version: config
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            input_spec: TensorSpec {
                name: "x".into(),
                shape: input_shape,
                dtype: TensorDtype::Float32,
            },
            output_spec: vec![TensorSpec {
                name: "output".into(),
                shape: vec![1, 0, 0],
                dtype: TensorDtype::Float32,
            }],
            artifact_paths: vec![],
        };

        Self {
            descriptor,
            model_path: None,
            keys_path: None,
        }
    }

    /// Set model and keys paths.
    pub fn with_paths(mut self, model: PathBuf, keys: PathBuf) -> Self {
        self.model_path = Some(model);
        self.keys_path = Some(keys);
        self
    }
}

impl ModelPackage for CrnnTextRecognizerPackage {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn create_executor(
        &self,
        runtime: Arc<dyn RuntimeBackend>,
    ) -> Result<Box<dyn ModelExecutor>> {
        Ok(Box::new(CrnnTextRecognizerExecutor {
            descriptor: self.descriptor.clone(),
            runtime,
            model_path: self.model_path.clone(),
            keys_path: self.keys_path.clone(),
            session: None,
            keys: Vec::new(),
            first_char_id: 0,
        }))
    }
}

/// Executor for CRNN CTC text recognition.
///
/// Input: `ModelInput` with RGB image bytes (name="image", shape=[H, W, 3])
/// Output: `ModelOutput::Text` with recognized text strings
struct CrnnTextRecognizerExecutor {
    descriptor: ModelDescriptor,
    runtime: Arc<dyn RuntimeBackend>,
    model_path: Option<PathBuf>,
    keys_path: Option<PathBuf>,
    session: Option<Arc<Box<dyn InferenceSession>>>,
    keys: Vec<String>,
    first_char_id: usize,
}

impl CrnnTextRecognizerExecutor {
    /// Ensure session and keys are loaded, creating from paths if needed.
    fn ensure_loaded(&mut self) -> Result<(&Arc<Box<dyn InferenceSession>>, &Vec<String>, usize)> {
        if self.session.is_some() && !self.keys.is_empty() {
            return Ok((
                self.session.as_ref().unwrap(),
                &self.keys,
                self.first_char_id,
            ));
        }

        let model_path = self.model_path.as_ref()
            .ok_or_else(|| SnipperError::Inference("No model path configured for CrnnTextRecognizer".into()))?;
        let keys_path = self.keys_path.as_ref()
            .ok_or_else(|| SnipperError::Inference("No keys path configured for CrnnTextRecognizer".into()))?;

        let handle = latexsnipper_runtime::ModelHandle::with_path(
            self.descriptor.id.composite_key(),
            model_path.to_path_buf(),
        );
        let session = self.runtime.create_session(&handle, AccelerationMode::Cpu)?;

        let (keys, first_char_id) = crate::text_recognizer::load_keys(keys_path)?;

        self.session = Some(Arc::new(session));
        self.keys = keys;
        self.first_char_id = first_char_id;

        Ok((
            self.session.as_ref().unwrap(),
            &self.keys,
            self.first_char_id,
        ))
    }
}

impl ModelExecutor for CrnnTextRecognizerExecutor {
    fn run(
        &mut self,
        input: ModelInput,
        _ctx: &mut InferenceContext,
    ) -> Result<ModelOutput> {
        let (session, keys, first_char_id) = self.ensure_loaded()?;

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
        let pixels: Vec<u8> = input.data.iter().copied().collect();

        let image = SnipperImage::new(
            width,
            height,
            latexsnipper_image::color::PixelFormat::Rgb,
            pixels,
        );

        // Run recognition using the existing inference function
        let params = crate::text_recognizer::TextRecParams::default();
        let result = crate::text_recognizer::recognize_text_with_keys(
            &image,
            &**session,
            keys,
            first_char_id,
            &params,
        )?;

        // Convert to ModelOutput
        Ok(ModelOutput::Text(vec![
            latexsnipper_runtime::TextResult {
                text: result.text,
                confidence: result.confidence,
            },
        ]))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}