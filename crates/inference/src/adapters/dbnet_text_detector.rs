use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    AccelerationMode, DetectionQuad, InferenceContext, InferenceSession, ModelDescriptor,
    ModelExecutor, ModelId, ModelInput, ModelOutput, ModelPackage, ModelTask, RuntimeBackend,
    TensorDtype, TensorSpec,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::text_detector::{detect_text, TextDetParams};
use crate::types::DetectionBox;

/// DBNet text detection model package.
///
/// Config-driven: reads input/output names, preprocessing, and DBNet postprocessing
/// parameters from config.json. Supports both PP-OCR and OpenOCR model variants.
pub struct DbNetTextDetectorPackage {
    descriptor: ModelDescriptor,
    model_path: Option<PathBuf>,
    params: TextDetParams,
}

impl DbNetTextDetectorPackage {
    /// Create from model config.
    pub fn from_config(config: &ModelConfig, model_id: ModelId) -> Self {
        let params = TextDetParams::from_config(config);

        let input_shape = config
            .input
            .as_ref()
            .map(|i| i.shape.iter().map(|s| *s as usize).collect())
            .unwrap_or_else(|| vec![1, 3, 960, 960]);

        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::TextDetection,
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
                shape: vec![1, 1, 0, 0],
                dtype: TensorDtype::Float32,
            }],
            artifact_paths: vec![],
        };

        Self {
            descriptor,
            model_path: None,
            params,
        }
    }

    /// Set the model file path.
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl ModelPackage for DbNetTextDetectorPackage {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>> {
        let model_path = self.model_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No model path configured for DbNetTextDetector".into())
        })?;

        Ok(Box::new(DbNetTextDetectorExecutor {
            descriptor: self.descriptor.clone(),
            params: self.params.clone(),
            runtime,
            model_path: model_path.clone(),
            session: None,
        }))
    }
}

/// Executor for DBNet text detection.
struct DbNetTextDetectorExecutor {
    descriptor: ModelDescriptor,
    params: TextDetParams,
    runtime: Arc<dyn RuntimeBackend>,
    model_path: PathBuf,
    session: Option<Arc<Box<dyn InferenceSession>>>,
}

impl DbNetTextDetectorExecutor {
    #[allow(clippy::unnecessary_unwrap)]
    fn ensure_loaded(&mut self) -> Result<&Arc<Box<dyn InferenceSession>>> {
        if self.session.is_some() {
            return Ok(self.session.as_ref().unwrap());
        }

        let handle = latexsnipper_runtime::ModelHandle::with_path(
            self.descriptor.id.composite_key(),
            self.model_path.clone(),
        );
        let session = self
            .runtime
            .create_session(&handle, AccelerationMode::Cpu)?;

        // Fail-fast: validate input name against actual session
        let input_names = session.input_names();
        if !input_names.iter().any(|n| n == &self.params.input_name) {
            return Err(SnipperError::Inference(format!(
                "DBNet input name '{}' not found in session inputs: {:?}. \
                 Check config.json 'input.name' field.",
                self.params.input_name, input_names
            )));
        }

        self.session = Some(Arc::new(session));
        Ok(self.session.as_ref().unwrap())
    }
}

impl ModelExecutor for DbNetTextDetectorExecutor {
    fn run(&mut self, input: ModelInput, _ctx: &mut InferenceContext) -> Result<ModelOutput> {
        let params = self.params.clone();
        let session = self.ensure_loaded()?.clone();

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

        let detections: Vec<DetectionBox> =
            detect_text(&image, session.as_ref().as_ref(), &params)?;

        let results: Vec<latexsnipper_runtime::DetectionResult> = detections
            .into_iter()
            .map(|d| latexsnipper_runtime::DetectionResult {
                x: d.rect.x,
                y: d.rect.y,
                width: d.rect.width,
                height: d.rect.height,
                quad: d.quad.map(|q| DetectionQuad {
                    x1: q.p1.x,
                    y1: q.p1.y,
                    x2: q.p2.x,
                    y2: q.p2.y,
                    x3: q.p3.x,
                    y3: q.p3.y,
                    x4: q.p4.x,
                    y4: q.p4.y,
                }),
                confidence: d.confidence,
                class_id: d.class_id,
                class_name: d.class_name,
            })
            .collect();

        Ok(ModelOutput::Detections(results))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}
