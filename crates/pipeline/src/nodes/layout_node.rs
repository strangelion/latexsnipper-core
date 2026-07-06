//! Layout analysis pipeline node.
//!
//! Runs the PP-DocLayout / PicoDet layout model on the page image and
//! emits region candidates into `PipelineArtifacts::region_candidates`.
//! These are then resolved by `RegionResolveNode`.

use async_trait::async_trait;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_runtime::{InferenceContext, ModelInput, ModelOutput, ModelTask};

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::get_backend;
use crate::region_graph::{RegionCandidate, RegionKind, RegionProducer};

/// Layout analysis node — optional; skipped when no layout model is installed.
pub struct LayoutNode {
    name: String,
}

impl LayoutNode {
    pub fn new() -> Self {
        Self {
            name: "layout_analysis".into(),
        }
    }
}

impl Default for LayoutNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for LayoutNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Try ModelPackage path first
        if let Some(package) = ctx.get_model_package(&ModelTask::LayoutAnalysis) {
            return self.analyze_via_package(ctx, &*package).await;
        }

        // No layout model — not an error, just skip
        log::info!("LayoutNode: no layout model available, skipping");
        Ok(())
    }
}

impl LayoutNode {
    async fn analyze_via_package(
        &self,
        ctx: &mut PipelineContext,
        package: &dyn latexsnipper_runtime::ModelPackage,
    ) -> Result<()> {
        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        let backend = get_backend(ctx)?;
        let mut executor = package.create_executor(backend)?;

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

        match output {
            ModelOutput::Layout(results) => {
                let page = ctx.current_page;
                let mut candidates = Vec::new();
                // Use a simple sequential id starting after any existing candidates
                let base_id = ctx.artifacts.region_candidates.len();

                for (i, r) in results.iter().enumerate() {
                    let kind = layout_label_to_region_kind(&r.region_type);
                    candidates.push(RegionCandidate {
                        id: base_id + i + 1,
                        kind,
                        rect: latexsnipper_ast::Rect::new(r.x, r.y, r.width, r.height),
                        quad: None,
                        confidence: r.confidence,
                        producer: RegionProducer::LayoutAnalysis,
                        page,
                    });
                }

                // Store in artifacts
                ctx.artifacts.region_candidates.extend(candidates);

                log::info!("LayoutNode: detected {} layout regions", results.len());
                Ok(())
            }
            _ => Err(SnipperError::Inference(
                "Unexpected output type from layout model".into(),
            )),
        }
    }
}

/// Map layout label strings to RegionKind.
fn layout_label_to_region_kind(label: &str) -> RegionKind {
    match label {
        "text" => RegionKind::TextParagraph,
        "title" => RegionKind::Heading,
        "figure" => RegionKind::Figure,
        "figure_caption" => RegionKind::Caption,
        "table" => RegionKind::Table,
        "table_caption" => RegionKind::Caption,
        "header" => RegionKind::Header,
        "footer" => RegionKind::Footer,
        "reference" => RegionKind::Unknown,
        "equation" => RegionKind::FormulaDisplay,
        _ => RegionKind::Unknown,
    }
}
