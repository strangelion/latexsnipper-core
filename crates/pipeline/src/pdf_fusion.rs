//! Explainable selection between native PDF and OCR-derived region candidates.

use std::collections::BTreeMap;

use latexsnipper_artifact::{
    ArtifactEdgeKind, ArtifactGraph, ArtifactId, ArtifactKind, ArtifactRecord,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfRegionSource {
    NativeText,
    OcrText,
    FormulaRecognizer,
    TableRecognizer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfRegionCandidate {
    pub page: usize,
    pub region_id: String,
    pub source: PdfRegionSource,
    pub content: String,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<ArtifactId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfFusionPolicy {
    pub native_text_min_confidence: f32,
    pub ocr_text_min_confidence: f32,
    pub specialized_min_confidence: f32,
}

impl Default for PdfFusionPolicy {
    fn default() -> Self {
        Self {
            native_text_min_confidence: 0.95,
            ocr_text_min_confidence: 0.80,
            specialized_min_confidence: 0.70,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfFusionReason {
    SpecializedFormula,
    SpecializedTable,
    NativeTextHighConfidence,
    OcrFallback,
    HighestConfidenceFallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfFusionDecision {
    pub selected: PdfRegionCandidate,
    pub rejected: Vec<PdfRegionCandidate>,
    pub reason: PdfFusionReason,
}

impl PdfFusionDecision {
    /// Add the fusion result and all source lineage to a runtime artifact graph.
    pub fn record_artifact_lineage(&self, graph: &mut ArtifactGraph) -> ArtifactId {
        let id = ArtifactId(format!(
            "pdf:fusion:{}:{}",
            self.selected.page, self.selected.region_id
        ));
        graph.insert(ArtifactRecord {
            id: id.clone(),
            kind: ArtifactKind::FusedRegion,
            stable_id: Some(self.selected.region_id.clone()),
            content_ref: Some(self.selected.content.clone()),
            checksum: None,
            provenance: Vec::new(),
        });
        for candidate in std::iter::once(&self.selected).chain(&self.rejected) {
            if let Some(source_id) = &candidate.artifact_id {
                graph.link(source_id.clone(), id.clone(), ArtifactEdgeKind::DerivedFrom);
            }
        }
        id
    }
}

/// Fuse candidates independently for each page-region pair.
pub fn fuse_pdf_regions(
    candidates: impl IntoIterator<Item = PdfRegionCandidate>,
    policy: PdfFusionPolicy,
) -> Vec<PdfFusionDecision> {
    let mut grouped: BTreeMap<(usize, String), Vec<PdfRegionCandidate>> = BTreeMap::new();
    for candidate in candidates {
        grouped
            .entry((candidate.page, candidate.region_id.clone()))
            .or_default()
            .push(candidate);
    }
    grouped
        .into_values()
        .filter_map(|group| select_region(group, policy))
        .collect()
}

fn select_region(
    candidates: Vec<PdfRegionCandidate>,
    policy: PdfFusionPolicy,
) -> Option<PdfFusionDecision> {
    let (index, reason) = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.confidence >= policy.specialized_min_confidence)
        .filter(|(_, candidate)| candidate.source == PdfRegionSource::FormulaRecognizer)
        .max_by(|(_, left), (_, right)| left.confidence.total_cmp(&right.confidence))
        .map(|(index, _)| (index, PdfFusionReason::SpecializedFormula))
        .or_else(|| {
            candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate.confidence >= policy.specialized_min_confidence)
                .filter(|(_, candidate)| candidate.source == PdfRegionSource::TableRecognizer)
                .max_by(|(_, left), (_, right)| left.confidence.total_cmp(&right.confidence))
                .map(|(index, _)| (index, PdfFusionReason::SpecializedTable))
        })
        .or_else(|| {
            candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    candidate.source == PdfRegionSource::NativeText
                        && candidate.confidence >= policy.native_text_min_confidence
                })
                .max_by(|(_, left), (_, right)| left.confidence.total_cmp(&right.confidence))
                .map(|(index, _)| (index, PdfFusionReason::NativeTextHighConfidence))
        })
        .or_else(|| {
            candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    candidate.source == PdfRegionSource::OcrText
                        && candidate.confidence >= policy.ocr_text_min_confidence
                })
                .max_by(|(_, left), (_, right)| left.confidence.total_cmp(&right.confidence))
                .map(|(index, _)| (index, PdfFusionReason::OcrFallback))
        })
        .or_else(|| {
            candidates
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.confidence.total_cmp(&right.confidence))
                .map(|(index, _)| (index, PdfFusionReason::HighestConfidenceFallback))
        })?;

    let selected = candidates[index].clone();
    let rejected = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(candidate_index, candidate)| (candidate_index != index).then_some(candidate))
        .collect();
    Some(PdfFusionDecision {
        selected,
        rejected,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(source: PdfRegionSource, confidence: f32) -> PdfRegionCandidate {
        PdfRegionCandidate {
            page: 0,
            region_id: "region-1".to_string(),
            source,
            content: "content".to_string(),
            confidence,
            artifact_id: None,
        }
    }

    #[test]
    fn formula_candidate_beats_native_text_for_the_same_region() {
        let decisions = fuse_pdf_regions(
            [
                candidate(PdfRegionSource::NativeText, 0.99),
                candidate(PdfRegionSource::FormulaRecognizer, 0.75),
            ],
            PdfFusionPolicy::default(),
        );
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].reason, PdfFusionReason::SpecializedFormula);
        assert_eq!(
            decisions[0].selected.source,
            PdfRegionSource::FormulaRecognizer
        );
    }

    #[test]
    fn fusion_records_all_available_candidate_artifacts() {
        let mut native = candidate(PdfRegionSource::NativeText, 0.99);
        native.artifact_id = Some(ArtifactId::from("native:1"));
        let mut ocr = candidate(PdfRegionSource::OcrText, 0.90);
        ocr.artifact_id = Some(ArtifactId::from("ocr:1"));
        let decision = fuse_pdf_regions([native, ocr], PdfFusionPolicy::default())
            .into_iter()
            .next()
            .unwrap();
        let mut graph = ArtifactGraph::default();
        let id = decision.record_artifact_lineage(&mut graph);
        assert!(graph.get(&id).is_some());
        assert_eq!(graph.edges().len(), 2);
    }
}
