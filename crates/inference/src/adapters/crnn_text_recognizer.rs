use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    AccelerationMode, InferenceContext, InferenceSession, ModelDescriptor, ModelExecutionContext,
    ModelExecutor, ModelId, ModelInput, ModelOutput, ModelPackage, ModelTask, RuntimeBackend,
    RuntimeSessionCompatibility, StubRuntime, TensorDtype, TensorSpec,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::text_recognizer::{load_keys, TextRecParams};

pub struct CrnnTextRecognizerPackage {
    descriptor: ModelDescriptor,
    model_path: Option<PathBuf>,
    keys_path: Option<PathBuf>,
    params: TextRecParams,
}

impl CrnnTextRecognizerPackage {
    pub fn from_config(config: &ModelConfig, model_id: ModelId) -> Self {
        let params = TextRecParams::from_config(config);
        let input_shape = config
            .input
            .as_ref()
            .map(|i| i.shape.iter().map(|s| *s as usize).collect())
            .unwrap_or_else(|| vec![1, 3, params.target_h as usize, params.max_w as usize]);
        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::TextRecognition,
            version: config
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            input_spec: TensorSpec {
                name: params.input_name.clone(),
                shape: input_shape,
                dtype: TensorDtype::Float32,
            },
            output_spec: vec![TensorSpec {
                name: params.output_name.clone(),
                shape: vec![1, 0, 0],
                dtype: TensorDtype::Float32,
            }],
            artifact_paths: vec![],
        };
        Self {
            descriptor,
            model_path: None,
            keys_path: None,
            params,
        }
    }

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

    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>> {
        Ok(Box::new(CrnnTextRecognizerExecutor::new(
            self.descriptor.clone(),
            self.params.clone(),
            self.model_path.clone(),
            self.keys_path.clone(),
            None,
            runtime,
        )))
    }

    fn create_executor_with_context(
        &self,
        ctx: &ModelExecutionContext,
    ) -> Result<Box<dyn ModelExecutor>> {
        let session = ctx.create_session("model")?;
        let compat: Arc<Box<dyn InferenceSession>> =
            Arc::new(Box::new(RuntimeSessionCompatibility::new(session)));
        Ok(Box::new(CrnnTextRecognizerExecutor::new(
            self.descriptor.clone(),
            self.params.clone(),
            self.model_path.clone(),
            self.keys_path.clone(),
            Some(compat),
            Arc::new(StubRuntime::new()),
        )))
    }
}

struct CrnnTextRecognizerExecutor {
    descriptor: ModelDescriptor,
    params: TextRecParams,
    model_path: Option<PathBuf>,
    keys_path: Option<PathBuf>,
    session: Option<Arc<Box<dyn InferenceSession>>>,
    _runtime: Arc<dyn RuntimeBackend>,
    keys: Vec<String>,
    first_char_id: usize,
}

impl CrnnTextRecognizerExecutor {
    fn new(
        descriptor: ModelDescriptor,
        params: TextRecParams,
        model_path: Option<PathBuf>,
        keys_path: Option<PathBuf>,
        session: Option<Arc<Box<dyn InferenceSession>>>,
        runtime: Arc<dyn RuntimeBackend>,
    ) -> Self {
        Self {
            descriptor,
            params,
            model_path,
            keys_path,
            session,
            _runtime: runtime,
            keys: Vec::new(),
            first_char_id: 0,
        }
    }

    fn ensure_loaded(&mut self) -> Result<()> {
        if self.session.is_some() && !self.keys.is_empty() {
            return Ok(());
        }
        let model_path = self.model_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No model path configured for CrnnTextRecognizer".into())
        })?;
        let keys_path = self.keys_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No keys path configured for CrnnTextRecognizer".into())
        })?;
        if self.session.is_none() {
            let handle = latexsnipper_runtime::ModelHandle::with_path(
                self.descriptor.id.composite_key(),
                model_path.to_path_buf(),
            );
            let session = self
                ._runtime
                .create_session(&handle, AccelerationMode::Cpu)?;
            self.session = Some(Arc::new(session));
        }
        let (keys, first_char_id) = load_keys(keys_path)?;
        self.keys = keys;
        self.first_char_id = first_char_id;
        Ok(())
    }
}

impl ModelExecutor for CrnnTextRecognizerExecutor {
    fn run(&mut self, input: ModelInput, _ctx: &mut InferenceContext) -> Result<ModelOutput> {
        self.ensure_loaded()?;
        let session = self.session.as_ref().unwrap();
        let shape = &input.shape;
        if shape.len() != 3 {
            return Err(SnipperError::Inference(format!(
                "Expected 3D shape [H, W, 3], got {:?}",
                shape
            )));
        }
        let height = shape[0] as u32;
        let width = shape[1] as u32;
        let image = SnipperImage::new(
            width,
            height,
            latexsnipper_image::color::PixelFormat::Rgb,
            input.data.to_vec(),
        );
        let result = crate::text_recognizer::recognize_text_with_keys(
            &image,
            &**session,
            &self.keys,
            self.first_char_id,
            &self.params,
        )?;
        Ok(ModelOutput::Text(vec![latexsnipper_runtime::TextResult {
            text: result.text,
            confidence: result.confidence,
        }]))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}
