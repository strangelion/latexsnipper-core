use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::formula_dominance::{decide_formula_dominance, FormulaBoxInput};
use crate::node::PipelineNode;

/// Runs after formula detection and before text detection.
///
/// When the image is formula-dominant (all detections isolated/display,
/// formula regions cover the real ink above the versioned threshold, and no
/// significant text ink outside formulae) the node skips TextDetection and
/// TextRecognition: it clears text detections and records the decision so
/// downstream stages run whole-image FormulaRecognition only.
pub struct FormulaDominanceNode {
    name: String,
}

impl FormulaDominanceNode {
    pub fn new() -> Self {
        Self {
            name: "formula_dominance".into(),
        }
    }
}

impl Default for FormulaDominanceNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for FormulaDominanceNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        let formula_detections = ctx.artifacts.formula_detections.clone();
        if formula_detections.is_empty() {
            return Ok(());
        }

        let boxes: Vec<FormulaBoxInput> = formula_detections
            .iter()
            .map(|det| FormulaBoxInput {
                rect: det.rect,
                isolated: true,
                confidence: det.confidence,
            })
            .collect();

        let decision = decide_formula_dominance(
            &image,
            &boxes,
            &crate::formula_dominance::FormulaDominancePolicy::default(),
        );

        let decision = match decision {
            Some(d) => d,
            None => return Ok(()),
        };

        if decision.dominant {
            // Formula-dominant fast path: skip text pipeline entirely.
            ctx.artifacts.text_detections.clear();
            ctx.metadata
                .insert("fastPath".into(), serde_json::json!("formulaDominant"));
            ctx.metadata.insert(
                "formulaDominanceDecision".into(),
                serde_json::to_value(&decision).unwrap_or_default(),
            );
            log::info!(
                "FormulaDominanceNode: formula-dominant fast path (boxes={}, coverage={:.3}, outside={:.3})",
                decision.formula_boxes,
                decision.ink_coverage,
                decision.outside_formula_ink
            );
        } else {
            ctx.metadata.insert(
                "formulaDominanceDecision".into(),
                serde_json::to_value(&decision).unwrap_or_default(),
            );
        }

        Ok(())
    }
}
