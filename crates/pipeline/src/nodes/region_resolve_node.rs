//! Region resolution node — collects all detections into a unified graph,
//! resolves conflicts, and projects results back into legacy artifact fields.
//!
//! Correctly imports existing `region_candidates` (from LayoutNode) and uses
//! `ArtifactRef` for correct legacy projection (instead of array indexing).

use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::region_graph::{ArtifactRef, RegionCandidate, RegionGraph, RegionKind, RegionProducer};

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

        // ── Step 3: project back to legacy fields using ArtifactRef ──────────
        // Collect which formula detection indices were discarded
        let mut discarded_formula = std::collections::HashSet::new();
        let mut discarded_text = std::collections::HashSet::new();

        for r in &resolved {
            if r.owner != crate::region_graph::RegionOwner::Discarded {
                continue;
            }
            match r.candidate.artifact_ref {
                ArtifactRef::FormulaDetection(idx) => {
                    discarded_formula.insert(idx);
                }
                ArtifactRef::TextDetection(idx) => {
                    discarded_text.insert(idx);
                }
                _ => {}
            }
        }

        ctx.artifacts.formula_detections = ctx
            .artifacts
            .formula_detections
            .drain(..)
            .enumerate()
            .filter(|(i, _)| !discarded_formula.contains(i))
            .map(|(_, d)| d)
            .collect();

        ctx.artifacts.text_detections = ctx
            .artifacts
            .text_detections
            .drain(..)
            .enumerate()
            .filter(|(i, _)| !discarded_text.contains(i))
            .map(|(_, d)| d)
            .collect();

        // ── Step 4: store resolved regions ─────────────────────────────────
        ctx.artifacts.resolved_regions = resolved;

        log::info!(
            "RegionResolveNode: discarded {} formula + {} text detections",
            discarded_formula.len(),
            discarded_text.len(),
        );

        Ok(())
    }
}
