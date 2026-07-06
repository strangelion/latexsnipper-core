//! Region resolution node — collects all detections into a unified graph,
//! resolves conflicts, and projects results back into legacy artifact fields.

use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::region_graph::{RegionGraph, RegionKind, RegionProducer};

/// Pipeline node that resolves region conflicts across detectors.
///
/// This node runs AFTER all detection nodes and BEFORE recognizer nodes.
/// It unifies text_detections, formula_detections, table_detections etc.
/// into a single region graph, resolves overlaps (table > formula > text),
/// and writes resolved regions into PipelineArtifacts.
///
/// Legacy vectors are preserved — downstream recognizers still read from
/// their original artifact fields during migration.
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

        // ── Step 1: collect all detection candidates ────────────────

        for det in &ctx.artifacts.formula_detections {
            graph.add_detection(det, RegionKind::FormulaDisplay, RegionProducer::FormulaDetector, page);
        }

        for det in &ctx.artifacts.text_detections {
            graph.add_detection(det, RegionKind::TextLine, RegionProducer::TextDetector, page);
        }

        for det in &ctx.artifacts.table_detections {
            graph.add_detection(det, RegionKind::Table, RegionProducer::TableDetector, page);
        }

        for det in &ctx.artifacts.handwriting_detections {
            graph.add_detection(det, RegionKind::Unknown, RegionProducer::HandwritingDetector, page);
        }

        for table in &ctx.artifacts.table_structures {
            graph.add_table(table, page);
        }

        // ── Step 2: resolve conflicts ───────────────────────────────

        let resolved = graph.resolve();

        // Store in artifacts
        let candidates: Vec<_> = graph.candidates().cloned().collect();
        ctx.artifacts.region_candidates = candidates;
        ctx.artifacts.resolved_regions = resolved;

        // ── Step 3: project back to legacy fields ───────────────────
        // This ensures downstream recognizers can still read from their
        // original artifact vectors during the migration period.

        // Filter out text detections that were discarded (overlapped by formula/table)
        let kept_text: Vec<_> = ctx
            .artifacts
            .text_detections
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                ctx.artifacts
                    .resolved_regions
                    .get(*i)
                    .map_or(true, |r| r.owner != crate::region_graph::RegionOwner::Discarded)
            })
            .map(|(_, d)| d.clone())
            .collect();

        // Replace text detections with only non-discarded ones
        ctx.artifacts.text_detections = kept_text;

        let kept_formula: Vec<_> = ctx
            .artifacts
            .formula_detections
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                ctx.artifacts
                    .resolved_regions
                    .get(*i)
                    .map_or(true, |r| r.owner != crate::region_graph::RegionOwner::Discarded)
            })
            .map(|(_, d)| d.clone())
            .collect();

        ctx.artifacts.formula_detections = kept_formula;

        log::info!(
            "RegionResolveNode: {} candidates → {} resolved regions",
            ctx.artifacts.region_candidates.len(),
            ctx.artifacts.resolved_regions.len(),
        );

        Ok(())
    }
}
