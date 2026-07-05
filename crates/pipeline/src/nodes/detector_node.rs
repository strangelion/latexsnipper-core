use async_trait::async_trait;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_inference::{
    detect_formulas, detect_handwriting, detect_tables, detect_text, filter_formula_detections,
    filter_handwriting_detections, filter_table_detections, group_formula_detections,
    DetectionParams, HandwritingDetParams, TableDetParams, TextDetParams,
};
use latexsnipper_runtime::{InferenceContext, ModelInput, ModelOutput, ModelTask};

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::{
    find_best_with_fallback, get_backend, get_or_create_session, load_config, resolve_model_handle,
};

/// Detects regions (formulas, text, handwriting, or tables) in the image.
/// Loads models, runs detection, stores results in context artifacts.
///
/// The `task` field determines which detection logic to use.
pub struct DetectorNode {
    name: String,
    task: ModelTask,
}

impl DetectorNode {
    /// Create a detector for a specific task.
    pub fn for_task(task: ModelTask) -> Self {
        let name = match task {
            ModelTask::FormulaDetection => "detect_formula".to_string(),
            ModelTask::TextDetection => "detect_text".to_string(),
            ModelTask::HandwritingRecognition => "detect_handwriting".to_string(),
            ModelTask::TableDetection => "detect_table".to_string(),
            _ => format!("detect_{:?}", task).to_lowercase(),
        };
        Self { name, task }
    }

    /// Create a formula detection node.
    pub fn formula() -> Self {
        Self::for_task(ModelTask::FormulaDetection)
    }

    /// Create a text detection node.
    pub fn text() -> Self {
        Self::for_task(ModelTask::TextDetection)
    }

    /// Create a handwriting detection node.
    pub fn handwriting() -> Self {
        Self::for_task(ModelTask::HandwritingRecognition)
    }

    /// Create a table detection node.
    pub fn table() -> Self {
        Self::for_task(ModelTask::TableDetection)
    }

    /// Get the model task this node performs.
    pub fn task(&self) -> ModelTask {
        self.task
    }
}

#[async_trait]
impl PipelineNode for DetectorNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        // Try using ModelPackage if available
        if let Some(package) = ctx.get_model_package(&self.task) {
            return self.detect_via_package(ctx, &image, &*package).await;
        }

        // Fall back to direct function calls
        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        match self.task {
            ModelTask::FormulaDetection => self.detect_formulas(ctx, &image, &models).await,
            ModelTask::TextDetection => self.detect_texts(ctx, &image, &models).await,
            ModelTask::HandwritingRecognition => {
                self.detect_handwriting(ctx, &image, &models).await
            }
            ModelTask::TableDetection => self.detect_tables(ctx, &image, &models).await,
            _ => {
                ctx.diagnostic_warn(
                    &self.name,
                    format!("Unsupported detection task: {:?}", self.task),
                );
                Ok(())
            }
        }
    }
}

impl DetectorNode {
    /// Detect using ModelPackage abstraction.
    async fn detect_via_package(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        package: &dyn latexsnipper_runtime::ModelPackage,
    ) -> Result<()> {
        let backend = get_backend(ctx)?;
        let mut executor = package.create_executor(backend)?;

        // Prepare input: RGB image bytes with shape [H, W, 3]
        let pixels = image.pixels().to_vec();
        let shape = vec![image.height() as usize, image.width() as usize, 3];
        let input = ModelInput {
            name: "image".to_string(),
            data: pixels,
            shape,
            dtype: latexsnipper_runtime::TensorDtype::UInt8,
        };

        let mut inf_ctx = InferenceContext::new();
        let output = executor.run(input, &mut inf_ctx)?;

        // Convert ModelOutput to detections
        match output {
            ModelOutput::Detections(results) => {
                let (class_id, class_name) = self.task_to_class();
                let detections: Vec<latexsnipper_inference::DetectionBox> = results
                    .into_iter()
                    .map(|r| latexsnipper_inference::DetectionBox {
                        rect: latexsnipper_ast::Rect::new(r.x, r.y, r.width, r.height),
                        confidence: r.confidence,
                        class_id,
                        class_name: class_name.clone(),
                    })
                    .collect();

                match self.task {
                    ModelTask::FormulaDetection => {
                        ctx.artifacts.formula_detections = detections;
                    }
                    ModelTask::TextDetection => {
                        ctx.artifacts.text_detections = detections;
                    }
                    ModelTask::HandwritingRecognition => {
                        ctx.artifacts.handwriting_detections = detections;
                    }
                    ModelTask::TableDetection => {
                        ctx.artifacts.table_detections = detections;
                    }
                    _ => {}
                }

                log::info!(
                    "Pipeline: {} found {} regions via ModelPackage",
                    self.name,
                    ctx.artifacts.formula_detections.len()
                        + ctx.artifacts.text_detections.len()
                        + ctx.artifacts.handwriting_detections.len()
                        + ctx.artifacts.table_detections.len()
                );
                Ok(())
            }
            _ => Err(SnipperError::Inference(
                "Unexpected output type from detection model".into(),
            )),
        }
    }

    fn task_to_class(&self) -> (usize, String) {
        match self.task {
            ModelTask::FormulaDetection => (0, "formula".to_string()),
            ModelTask::TextDetection => (1, "text".to_string()),
            ModelTask::HandwritingRecognition => (2, "handwriting".to_string()),
            ModelTask::TableDetection => (3, "table".to_string()),
            _ => (0, "unknown".to_string()),
        }
    }

    async fn detect_formulas(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(ctx, models, "formula-det") {
            Ok(c) => c,
            Err(_) => {
                ctx.diagnostic_warn(
                    "detect_formula",
                    "Formula detection model config not found, skipping",
                );
                return Ok(());
            }
        };

        let det_params = DetectionParams::from_config(&det_config);

        let (variant_config, det_model_path, _variant_dir) =
            latexsnipper_model::ModelConfig::find_best(models, "formula-det")
                .ok_or_else(|| SnipperError::Model("Formula detection model not found".into()))?;
        let det_handle = resolve_model_handle(ctx, "formula-det", det_model_path)?;

        let backend = get_backend(ctx)?;
        let session = get_or_create_session(ctx, "formula_det", &backend, &det_handle)?;

        let mut detections = detect_formulas(image, &*session, &det_params)?;

        group_formula_detections(&mut detections);

        let min_area = variant_config.pipeline_min_area();
        let min_conf = variant_config.pipeline_min_confidence();
        filter_formula_detections(&mut detections, min_area, min_conf);

        let count = detections.len();
        log::info!(
            "Pipeline: detect_formula found {} regions after grouping",
            count
        );

        ctx.artifacts.formula_detections = detections;
        Ok(())
    }

    async fn detect_texts(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(ctx, models, "text-det") {
            Ok(c) => c,
            Err(_) => {
                ctx.diagnostic_warn(
                    "detect_text",
                    "Text detection model config not found, skipping",
                );
                return Ok(());
            }
        };

        let det_params = TextDetParams::default();

        let (_variant_config, det_model_path, _variant_dir) =
            find_best_with_fallback(models, "text-det", &det_config)
                .ok_or_else(|| SnipperError::Model("Text detection model not found".into()))?;

        let det_handle = resolve_model_handle(ctx, "text-det", det_model_path)?;

        let backend = get_backend(ctx)?;
        let session = get_or_create_session(ctx, "text_det", &backend, &det_handle)?;

        let detections = detect_text(image, &*session, &det_params)?;
        let count = detections.len();
        log::info!("Pipeline: detect_text found {} regions", count);

        ctx.artifacts.text_detections = detections;
        Ok(())
    }

    async fn detect_handwriting(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(ctx, models, "handwriting-det") {
            Ok(c) => c,
            Err(_) => {
                ctx.diagnostic_warn(
                    "detect_handwriting",
                    "Handwriting detection model config not found, skipping",
                );
                return Ok(());
            }
        };

        let det_params = HandwritingDetParams::from_config(&det_config);

        let (variant_config, det_model_path, _variant_dir) =
            latexsnipper_model::ModelConfig::find_best(models, "handwriting-det").ok_or_else(
                || SnipperError::Model("Handwriting detection model not found".into()),
            )?;
        let det_handle = resolve_model_handle(ctx, "handwriting-det", det_model_path)?;

        let backend = get_backend(ctx)?;
        let session = get_or_create_session(ctx, "handwriting_det", &backend, &det_handle)?;

        let mut detections = detect_handwriting(image, &*session, &det_params)?;

        let min_area = variant_config.pipeline_min_area();
        let min_conf = variant_config.pipeline_min_confidence();
        filter_handwriting_detections(&mut detections, min_area, min_conf);

        let count = detections.len();
        log::info!("Pipeline: detect_handwriting found {} regions", count);

        ctx.artifacts.handwriting_detections = detections;
        Ok(())
    }

    async fn detect_tables(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        models: &std::path::Path,
    ) -> Result<()> {
        let det_config = match load_config(ctx, models, "table-det") {
            Ok(c) => c,
            Err(_) => {
                ctx.diagnostic_warn(
                    "detect_table",
                    "Table detection model config not found, skipping",
                );
                return Ok(());
            }
        };

        let det_params = TableDetParams::from_config(&det_config);

        let (variant_config, det_model_path, _variant_dir) =
            latexsnipper_model::ModelConfig::find_best(models, "table-det")
                .ok_or_else(|| SnipperError::Model("Table detection model not found".into()))?;
        let det_handle = resolve_model_handle(ctx, "table-det", det_model_path)?;

        let backend = get_backend(ctx)?;
        let session = get_or_create_session(ctx, "table_det", &backend, &det_handle)?;

        let mut detections = detect_tables(image, &*session, &det_params)?;

        let min_area = variant_config.pipeline_min_area();
        let min_conf = variant_config.pipeline_min_confidence();
        filter_table_detections(&mut detections, min_area, min_conf);

        let count = detections.len();
        log::info!("Pipeline: detect_table found {} regions", count);

        ctx.artifacts.table_detections = detections;
        Ok(())
    }
}
