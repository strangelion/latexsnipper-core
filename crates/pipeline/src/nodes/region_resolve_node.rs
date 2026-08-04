//! Region resolution node — collects all detections into a unified graph,
//! resolves conflicts, and projects results back into legacy artifact fields.
//!
//! Correctly imports existing `region_candidates` (from LayoutNode) and uses
//! `ArtifactRef` for correct legacy projection (instead of array indexing).

use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::region_graph::{
    ArtifactRef, RecognitionTarget, RegionCandidate, RegionGraph, RegionKind, RegionProducer,
};
use latexsnipper_inference::DetectionBox;

/// Pipeline node that resolves region conflicts across detectors.
pub struct RegionResolveNode {
    name: String,
}

impl RegionResolveNode {
    pub fn new() -> Self {
        Self {
            name: "region_resolve".into(),
        }
    }
}

impl Default for RegionResolveNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for RegionResolveNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let page = ctx.current_page;
        let mut graph = RegionGraph::new();

        // ── Step 1a: import pre-existing region_candidates (from LayoutNode) ──
        for candidate in ctx.artifacts.region_candidates.drain(..) {
            graph.add_candidate(candidate);
        }

        // ── Step 1b: add detection candidates with ArtifactRef ───────────────
        for (idx, det) in ctx.artifacts.formula_detections.iter().enumerate() {
            graph.add_candidate(RegionCandidate {
                id: 0,
                kind: RegionKind::FormulaDisplay,
                rect: det.rect,
                quad: det.quad,
                confidence: det.confidence,
                producer: RegionProducer::FormulaDetector,
                page,
                artifact_ref: ArtifactRef::FormulaDetection(idx),
            });
        }
        for (idx, det) in ctx.artifacts.text_detections.iter().enumerate() {
            graph.add_candidate(RegionCandidate {
                id: 0,
                kind: RegionKind::TextLine,
                rect: det.rect,
                quad: det.quad,
                confidence: det.confidence,
                producer: RegionProducer::TextDetector,
                page,
                artifact_ref: ArtifactRef::TextDetection(idx),
            });
        }
        for (idx, det) in ctx.artifacts.table_detections.iter().enumerate() {
            graph.add_candidate(RegionCandidate {
                id: 0,
                kind: RegionKind::Table,
                rect: det.rect,
                quad: det.quad,
                confidence: det.confidence,
                producer: RegionProducer::TableDetector,
                page,
                artifact_ref: ArtifactRef::TableDetection(idx),
            });
        }
        for (idx, det) in ctx.artifacts.handwriting_detections.iter().enumerate() {
            graph.add_candidate(RegionCandidate {
                id: 0,
                kind: RegionKind::Unknown,
                rect: det.rect,
                quad: det.quad,
                confidence: det.confidence,
                producer: RegionProducer::HandwritingDetector,
                page,
                artifact_ref: ArtifactRef::HandwritingDetection(idx),
            });
        }
        for (tbl_idx, table) in ctx.artifacts.table_structures.iter().enumerate() {
            graph.add_candidate(RegionCandidate {
                id: 0,
                kind: RegionKind::Table,
                rect: table.table_rect,
                quad: None,
                confidence: 1.0,
                producer: RegionProducer::TableDetector,
                page,
                artifact_ref: ArtifactRef::LayoutRegion(tbl_idx),
            });
            for (cell_idx, cell) in table.cells.iter().enumerate() {
                graph.add_candidate(RegionCandidate {
                    id: 0,
                    kind: RegionKind::TableCell,
                    rect: cell.rect,
                    quad: None,
                    confidence: 1.0,
                    producer: RegionProducer::TableDetector,
                    page,
                    artifact_ref: ArtifactRef::TableCell {
                        table: tbl_idx,
                        cell: cell_idx,
                    },
                });
            }
        }

        // ── Step 2: resolve conflicts ───────────────────────────────────────
        let resolved = graph.resolve();

        // ── Step 3+4: build recognition targets and rewrite legacy detections ──
        // Only Independent regions become top-level recognition targets.
        // Child regions (table cell children) are recognized by the table path.
        // Text regions split around formulae project their surviving fragments
        // as independent text detections; the original (formula-contaminated)
        // detection is dropped. This keeps `recognition_targets` indexes
        // consistent with the rewritten `text_detections` list.
        let mut targets = Vec::new();
        let mut new_text_detections = Vec::new();
        let mut new_formula_detections = Vec::new();

        for r in &resolved {
            if r.owner != crate::region_graph::RegionOwner::Independent {
                continue;
            }
            match r.candidate.artifact_ref {
                ArtifactRef::TextDetection(idx) => {
                    let original = ctx.artifacts.text_detections.get(idx).cloned();
                    if !r.fragments.is_empty() {
                        // Region was split around formulae: only surviving
                        // fragments become recognition targets.
                        for f in r.fragments.iter().filter(|f| {
                            !matches!(
                                f.provenance,
                                crate::region_graph::RegionFragmentProvenance::RemovedTooSmall { .. }
                            )
                        }) {
                            let slot = new_text_detections.len();
                            targets.push(RecognitionTarget::TopLevelText {
                                detection_index: slot,
                            });
                            // Preserve the clipped quad so downstream crop/
                            // warp stages keep supporting rotated text.
                            let mut det = DetectionBox::rect(
                                f.rect,
                                r.candidate.confidence,
                                0,
                                "text".into(),
                            );
                            det.quad = f.polygon;
                            new_text_detections.push(det);
                        }
                        let _ = original;
                    } else if let Some(det) = original {
                        let slot = new_text_detections.len();
                        targets.push(RecognitionTarget::TopLevelText {
                            detection_index: slot,
                        });
                        new_text_detections.push(det);
                    }
                }
                ArtifactRef::FormulaDetection(idx) => {
                    if let Some(det) = ctx.artifacts.formula_detections.get(idx).cloned() {
                        let slot = new_formula_detections.len();
                        targets.push(RecognitionTarget::TopLevelFormula {
                            detection_index: slot,
                        });
                        new_formula_detections.push(det);
                    }
                }
                ArtifactRef::HandwritingDetection(idx) => {
                    targets.push(RecognitionTarget::TopLevelHandwriting {
                        detection_index: idx,
                    });
                }
                // TableCell targets are added separately for table recognizer
                _ => {}
            }
        }

        // Add table cell recognition targets (these are handled by TableRecognizerNode)
        for (tbl_idx, table) in ctx.artifacts.table_structures.iter().enumerate() {
            for cell_idx in 0..table.cells.len() {
                targets.push(RecognitionTarget::TableCell {
                    table_index: tbl_idx,
                    cell_index: cell_idx,
                });
            }
        }

        ctx.artifacts.text_detections = new_text_detections;
        ctx.artifacts.formula_detections = new_formula_detections;

        // ── Step 5: store resolved regions and targets ─────────────────────
        ctx.artifacts.resolved_regions = resolved;
        ctx.artifacts.recognition_targets = targets;

        log::info!(
            "RegionResolveNode: {} recognition targets generated",
            ctx.artifacts.recognition_targets.len(),
        );

        Ok(())
    }
}
