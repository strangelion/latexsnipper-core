use async_trait::async_trait;
use latexsnipper_foundation::Result;
use latexsnipper_inference::DetectionBox;

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
                // Read the real isolated/display label from the detector
                // instead of assuming every detection is display mode.
                isolated: det.class_name == "isolated" || det.class_name == "display",
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
            // Formula-dominant fast path: skip text detection entirely and
            // replace per-box detections with a single whole-image formula
            // detection, so RecognizeFormula runs whole-image recognition
            // and produces one FormulaBlock (fastPath = formulaDominant).
            ctx.artifacts.text_detections.clear();
            let full_rect =
                latexsnipper_ast::Rect::new(0.0, 0.0, image.width() as f32, image.height() as f32);
            let best_confidence = formula_detections
                .iter()
                .map(|det| det.confidence)
                .fold(0.0f32, f32::max)
                .max(0.9);
            ctx.artifacts.formula_detections = vec![DetectionBox::rect(
                full_rect,
                best_confidence,
                1,
                "isolated".into(),
            )];
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
