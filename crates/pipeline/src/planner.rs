use crate::{
    DocumentParseMode, PipelineCapability, PipelineDependency, PipelineNodeSpec, PipelinePlan,
    PipelineProfile,
};

/// Builds deterministic plans from controlled profiles and parse modes.
#[derive(Debug, Default, Clone, Copy)]
pub struct PipelinePlanner;

impl PipelinePlanner {
    pub fn plan(self, profile: PipelineProfile, parse_mode: DocumentParseMode) -> PipelinePlan {
        match profile {
            PipelineProfile::Formula => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::DetectFormula,
                    PipelineNodeSpec::Crop,
                    PipelineNodeSpec::RecognizeFormula,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![
                    dependency(PipelineNodeSpec::Crop, &[PipelineNodeSpec::DetectFormula]),
                    dependency(
                        PipelineNodeSpec::RecognizeFormula,
                        &[PipelineNodeSpec::Crop],
                    ),
                    dependency(
                        PipelineNodeSpec::Postprocess,
                        &[PipelineNodeSpec::RecognizeFormula],
                    ),
                ],
                vec![
                    PipelineCapability::FormulaDetection,
                    PipelineCapability::FormulaRecognition,
                ],
            ),
            PipelineProfile::CroppedFormula => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::RecognizeFormula,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![dependency(
                    PipelineNodeSpec::Postprocess,
                    &[PipelineNodeSpec::RecognizeFormula],
                )],
                vec![PipelineCapability::FormulaRecognition],
            ),
            PipelineProfile::Text => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::DetectText,
                    PipelineNodeSpec::Crop,
                    PipelineNodeSpec::RecognizeText,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![
                    dependency(PipelineNodeSpec::Crop, &[PipelineNodeSpec::DetectText]),
                    dependency(PipelineNodeSpec::RecognizeText, &[PipelineNodeSpec::Crop]),
                    dependency(
                        PipelineNodeSpec::Postprocess,
                        &[PipelineNodeSpec::RecognizeText],
                    ),
                ],
                vec![
                    PipelineCapability::TextDetection,
                    PipelineCapability::TextRecognition,
                ],
            ),
            PipelineProfile::Mixed if parse_mode == DocumentParseMode::OpenDocHybrid => {
                hybrid_plan(profile, parse_mode)
            }
            PipelineProfile::Mixed => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::DetectFormula,
                    PipelineNodeSpec::CheckFormulaDominance,
                    PipelineNodeSpec::DetectText,
                    PipelineNodeSpec::Crop,
                    PipelineNodeSpec::RecognizeFormula,
                    PipelineNodeSpec::RecognizeText,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![
                    dependency(
                        PipelineNodeSpec::Crop,
                        &[
                            PipelineNodeSpec::DetectFormula,
                            PipelineNodeSpec::DetectText,
                        ],
                    ),
                    dependency(
                        PipelineNodeSpec::CheckFormulaDominance,
                        &[PipelineNodeSpec::DetectFormula],
                    ),
                    dependency(
                        PipelineNodeSpec::DetectText,
                        &[PipelineNodeSpec::CheckFormulaDominance],
                    ),
                    dependency(
                        PipelineNodeSpec::RecognizeFormula,
                        &[PipelineNodeSpec::Crop],
                    ),
                    dependency(PipelineNodeSpec::RecognizeText, &[PipelineNodeSpec::Crop]),
                    dependency(
                        PipelineNodeSpec::Postprocess,
                        &[
                            PipelineNodeSpec::RecognizeFormula,
                            PipelineNodeSpec::RecognizeText,
                        ],
                    ),
                ],
                vec![
                    PipelineCapability::FormulaDetection,
                    PipelineCapability::FormulaRecognition,
                    PipelineCapability::TextDetection,
                    PipelineCapability::TextRecognition,
                ],
            ),
            PipelineProfile::Handwriting => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::DetectHandwriting,
                    PipelineNodeSpec::Crop,
                    PipelineNodeSpec::RecognizeHandwriting,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![
                    dependency(
                        PipelineNodeSpec::Crop,
                        &[PipelineNodeSpec::DetectHandwriting],
                    ),
                    dependency(
                        PipelineNodeSpec::RecognizeHandwriting,
                        &[PipelineNodeSpec::Crop],
                    ),
                    dependency(
                        PipelineNodeSpec::Postprocess,
                        &[PipelineNodeSpec::RecognizeHandwriting],
                    ),
                ],
                vec![
                    PipelineCapability::HandwritingDetection,
                    PipelineCapability::HandwritingRecognition,
                ],
            ),
            PipelineProfile::Table => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::DetectTable,
                    PipelineNodeSpec::TableStructure,
                    PipelineNodeSpec::RecognizeTable,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![
                    dependency(
                        PipelineNodeSpec::TableStructure,
                        &[PipelineNodeSpec::DetectTable],
                    ),
                    dependency(
                        PipelineNodeSpec::RecognizeTable,
                        &[PipelineNodeSpec::TableStructure],
                    ),
                    dependency(
                        PipelineNodeSpec::Postprocess,
                        &[PipelineNodeSpec::RecognizeTable],
                    ),
                ],
                vec![
                    PipelineCapability::TableDetection,
                    PipelineCapability::TableRecognition,
                ],
            ),
            PipelineProfile::FormulaLayout => simple_plan(
                profile,
                parse_mode,
                vec![
                    PipelineNodeSpec::DetectFormula,
                    PipelineNodeSpec::Crop,
                    PipelineNodeSpec::RecognizeFormula,
                    PipelineNodeSpec::FormulaLayout,
                    PipelineNodeSpec::Postprocess,
                ],
                vec![
                    dependency(PipelineNodeSpec::Crop, &[PipelineNodeSpec::DetectFormula]),
                    dependency(
                        PipelineNodeSpec::RecognizeFormula,
                        &[PipelineNodeSpec::Crop],
                    ),
                    dependency(
                        PipelineNodeSpec::FormulaLayout,
                        &[PipelineNodeSpec::RecognizeFormula],
                    ),
                    dependency(
                        PipelineNodeSpec::Postprocess,
                        &[PipelineNodeSpec::FormulaLayout],
                    ),
                ],
                vec![
                    PipelineCapability::FormulaDetection,
                    PipelineCapability::FormulaRecognition,
                    PipelineCapability::FormulaLayout,
                ],
            ),
        }
    }
}

fn hybrid_plan(profile: PipelineProfile, parse_mode: DocumentParseMode) -> PipelinePlan {
    simple_plan(
        profile,
        parse_mode,
        vec![
            PipelineNodeSpec::Layout,
            PipelineNodeSpec::DetectFormula,
            PipelineNodeSpec::CheckFormulaDominance,
            PipelineNodeSpec::DetectText,
            PipelineNodeSpec::DetectTable,
            PipelineNodeSpec::Crop,
            PipelineNodeSpec::ResolveRegions,
            PipelineNodeSpec::RecognizeFormula,
            PipelineNodeSpec::RecognizeText,
            PipelineNodeSpec::TableStructure,
            PipelineNodeSpec::RecognizeTable,
            PipelineNodeSpec::Postprocess,
        ],
        vec![
            dependency(
                PipelineNodeSpec::Crop,
                &[
                    PipelineNodeSpec::DetectFormula,
                    PipelineNodeSpec::DetectText,
                ],
            ),
            dependency(
                PipelineNodeSpec::CheckFormulaDominance,
                &[PipelineNodeSpec::DetectFormula],
            ),
            dependency(
                PipelineNodeSpec::DetectText,
                &[PipelineNodeSpec::CheckFormulaDominance],
            ),
            dependency(
                PipelineNodeSpec::ResolveRegions,
                &[
                    PipelineNodeSpec::Layout,
                    PipelineNodeSpec::Crop,
                    PipelineNodeSpec::DetectFormula,
                    PipelineNodeSpec::DetectText,
                    PipelineNodeSpec::DetectTable,
                ],
            ),
            dependency(
                PipelineNodeSpec::RecognizeFormula,
                &[PipelineNodeSpec::ResolveRegions],
            ),
            dependency(
                PipelineNodeSpec::RecognizeText,
                &[PipelineNodeSpec::ResolveRegions],
            ),
            dependency(
                PipelineNodeSpec::TableStructure,
                &[PipelineNodeSpec::ResolveRegions],
            ),
            dependency(
                PipelineNodeSpec::RecognizeTable,
                &[PipelineNodeSpec::TableStructure],
            ),
            dependency(
                PipelineNodeSpec::Postprocess,
                &[
                    PipelineNodeSpec::RecognizeFormula,
                    PipelineNodeSpec::RecognizeText,
                    PipelineNodeSpec::RecognizeTable,
                ],
            ),
        ],
        vec![
            PipelineCapability::LayoutAnalysis,
            PipelineCapability::FormulaDetection,
            PipelineCapability::FormulaRecognition,
            PipelineCapability::TextDetection,
            PipelineCapability::TextRecognition,
            PipelineCapability::TableDetection,
            PipelineCapability::TableStructure,
            PipelineCapability::TableRecognition,
        ],
    )
}

fn simple_plan(
    profile: PipelineProfile,
    parse_mode: DocumentParseMode,
    nodes: Vec<PipelineNodeSpec>,
    edges: Vec<PipelineDependency>,
    required_capabilities: Vec<PipelineCapability>,
) -> PipelinePlan {
    PipelinePlan {
        id: profile.pipeline_name().to_string(),
        parse_mode,
        nodes,
        edges,
        required_capabilities,
    }
}

fn dependency(node: PipelineNodeSpec, depends_on: &[PipelineNodeSpec]) -> PipelineDependency {
    PipelineDependency {
        node,
        depends_on: depends_on.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_profile_is_declarative_and_preserves_existing_graph_shape() {
        let plan = PipelinePlanner.plan(PipelineProfile::Mixed, DocumentParseMode::OpenDocHybrid);
        assert_eq!(plan.id, "Mixed_pipeline");
        assert_eq!(plan.nodes.len(), 12);
        assert!(plan.nodes.contains(&PipelineNodeSpec::ResolveRegions));
        assert!(plan
            .nodes
            .contains(&PipelineNodeSpec::CheckFormulaDominance));
        assert_eq!(plan.build_graph().len(), 12);
    }

    #[test]
    fn formula_layout_profile_has_a_formula_layout_stage() {
        let plan = PipelinePlanner.plan(
            PipelineProfile::FormulaLayout,
            DocumentParseMode::SpecializedStable,
        );
        assert!(plan.nodes.contains(&PipelineNodeSpec::FormulaLayout));
        assert!(plan
            .required_capabilities
            .contains(&PipelineCapability::FormulaLayout));
    }

    #[test]
    fn cropped_formula_skips_detection_and_crop() {
        let plan = PipelinePlanner.plan(
            PipelineProfile::CroppedFormula,
            DocumentParseMode::SpecializedStable,
        );
        assert_eq!(
            plan.nodes,
            vec![
                PipelineNodeSpec::RecognizeFormula,
                PipelineNodeSpec::Postprocess
            ]
        );
        assert!(!plan.nodes.contains(&PipelineNodeSpec::DetectFormula));
        assert!(!plan.nodes.contains(&PipelineNodeSpec::Crop));
        assert_eq!(
            plan.required_capabilities,
            vec![PipelineCapability::FormulaRecognition]
        );
        assert!(
            plan.nodes.len()
                < PipelinePlanner
                    .plan(
                        PipelineProfile::Formula,
                        DocumentParseMode::SpecializedStable
                    )
                    .nodes
                    .len()
        );
    }

    #[test]
    fn mixed_plan_gates_text_detection_on_formula_dominance() {
        let plan =
            PipelinePlanner.plan(PipelineProfile::Mixed, DocumentParseMode::SpecializedStable);
        let formula_idx = plan
            .nodes
            .iter()
            .position(|n| *n == PipelineNodeSpec::DetectFormula)
            .unwrap();
        let dominance_idx = plan
            .nodes
            .iter()
            .position(|n| *n == PipelineNodeSpec::CheckFormulaDominance)
            .unwrap();
        let text_idx = plan
            .nodes
            .iter()
            .position(|n| *n == PipelineNodeSpec::DetectText)
            .unwrap();
        assert!(
            formula_idx < dominance_idx && dominance_idx < text_idx,
            "dominance check must run after formula detection and gate text detection"
        );
        // The dominance gate edge must exist.
        assert!(plan.edges.iter().any(|e| {
            e.node == PipelineNodeSpec::CheckFormulaDominance
                && e.depends_on.contains(&PipelineNodeSpec::DetectFormula)
        }));
        assert!(plan.edges.iter().any(|e| {
            e.node == PipelineNodeSpec::DetectText
                && e.depends_on
                    .contains(&PipelineNodeSpec::CheckFormulaDominance)
        }));
    }
}
