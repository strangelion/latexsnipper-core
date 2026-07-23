use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::SnipperImage;
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{
    InferenceContext, InferenceSession, ModelDescriptor, ModelExecutionContext, ModelExecutor,
    ModelId, ModelInput, ModelOutput, ModelPackage, ModelTask, RuntimeBackend,
    RuntimeSessionCompatibility, TensorDtype, TensorSpec,
};
use std::sync::Arc;

use crate::formula_detector::DetectionParams;

/// YOLOv8 formula detection model package.
pub struct YoloV8DetectorPackage {
    descriptor: ModelDescriptor,
    params: DetectionParams,
    model_path: Option<std::path::PathBuf>,
}

impl YoloV8DetectorPackage {
    pub fn from_config(config: &ModelConfig, model_id: ModelId) -> Self {
        let params = DetectionParams::from_config(config);
        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::FormulaDetection,
            version: config
                .model_family
                .clone()
                .unwrap_or_else(|| "unknown".into()),
            input_spec: TensorSpec {
                name: "images".into(),
                shape: vec![
                    1,
                    3,
                    params.target_size as usize,
                    params.target_size as usize,
                ],
                dtype: TensorDtype::Float32,
            },
            output_spec: vec![TensorSpec {
                name: "output".into(),
                shape: vec![1, 6, 8400],
                dtype: TensorDtype::Float32,
            }],
            artifact_paths: vec![],
        };
        Self {
            descriptor,
            params,
            model_path: None,
        }
    }

    pub fn with_params(params: DetectionParams, model_id: ModelId) -> Self {
        let descriptor = ModelDescriptor {
            id: model_id,
            task: ModelTask::FormulaDetection,
            version: "custom".into(),
            input_spec: TensorSpec {
                name: "images".into(),
                shape: vec![
                    1,
                    3,
                    params.target_size as usize,
                    params.target_size as usize,
                ],
                dtype: TensorDtype::Float32,
            },
            output_spec: vec![TensorSpec {
                name: "output".into(),
                shape: vec![1, 6, 8400],
                dtype: TensorDtype::Float32,
            }],
            artifact_paths: vec![],
        };
        Self {
            descriptor,
            params,
            model_path: None,
        }
    }

    pub fn with_model_path(mut self, path: std::path::PathBuf) -> Self {
        self.model_path = Some(path);
        self
    }
}

impl ModelPackage for YoloV8DetectorPackage {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn create_executor(&self, runtime: Arc<dyn RuntimeBackend>) -> Result<Box<dyn ModelExecutor>> {
        Ok(Box::new(YoloV8DetectorExecutor {
            descriptor: self.descriptor.clone(),
            params: self.params.clone(),
            model_path: self.model_path.clone(),
            session: None,
            _runtime: runtime,
        }))
    }

    fn create_executor_with_context(
        &self,
        ctx: &ModelExecutionContext,
    ) -> Result<Box<dyn ModelExecutor>> {
        let session = ctx.create_session("model")?;
        Ok(Box::new(YoloV8DetectorExecutor {
            descriptor: self.descriptor.clone(),
            params: self.params.clone(),
            model_path: self.model_path.clone(),
            session: Some(Arc::new(Box::new(RuntimeSessionCompatibility::new(
                session,
            )))),
            _runtime: Arc::new(latexsnipper_runtime::StubRuntime::new()),
        }))
    }
}

struct YoloV8DetectorExecutor {
    descriptor: ModelDescriptor,
    params: DetectionParams,
    model_path: Option<std::path::PathBuf>,
    session: Option<Arc<Box<dyn InferenceSession>>>,
    _runtime: Arc<dyn RuntimeBackend>,
}

impl YoloV8DetectorExecutor {
    fn ensure_session(&mut self) -> Result<&Arc<Box<dyn InferenceSession>>> {
        if let Some(ref session) = self.session {
            return Ok(session);
        }
        let model_path = self.model_path.as_ref().ok_or_else(|| {
            SnipperError::Inference("No model path configured for YoloV8Detector".into())
        })?;
        let handle = latexsnipper_runtime::ModelHandle::with_path(
            self.descriptor.id.composite_key(),
            model_path.to_path_buf(),
        );
        let session = self
            ._runtime
            .create_session(&handle, latexsnipper_runtime::AccelerationMode::Cpu)?;
        self.session = Some(Arc::new(session));
        Ok(self.session.as_ref().unwrap())
    }
}

impl ModelExecutor for YoloV8DetectorExecutor {
    fn run(&mut self, input: ModelInput, _ctx: &mut InferenceContext) -> Result<ModelOutput> {
        let params = self.params.clone();
        let session = self.ensure_session()?;
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
        let detections = crate::formula_detector::detect_formulas(&image, &**session, &params)?;
        let results: Vec<latexsnipper_runtime::DetectionResult> = detections
            .into_iter()
            .map(|d| latexsnipper_runtime::DetectionResult {
                x: d.rect.x,
                y: d.rect.y,
                width: d.rect.width,
                height: d.rect.height,
                quad: d.quad.map(|q| latexsnipper_runtime::DetectionQuad {
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
                class_id: 0,
                class_name: "formula".to_string(),
            })
            .collect();
        Ok(ModelOutput::Detections(results))
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }
}
