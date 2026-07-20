use latexsnipper_artifact::{ArtifactGraph, ArtifactTrace};
use latexsnipper_ast::{Block, Rect};
use latexsnipper_image::SnipperImage;
use latexsnipper_inference::{DetectionBox, GridCell};

use crate::region_graph::{RecognitionTarget, RegionCandidate, ResolvedRegion};

/// Strongly-typed pipeline data artifacts.
/// Replaces string-keyed metadata for type safety.
#[derive(Debug, Clone, Default)]
pub struct PipelineArtifacts {
    /// Runtime-only lineage for debugging, evaluation, and incremental reuse.
    pub artifact_graph: ArtifactGraph,
    // Detections (from detector nodes)
    pub formula_detections: Vec<DetectionBox>,
    pub text_detections: Vec<DetectionBox>,
    pub handwriting_detections: Vec<DetectionBox>,
    pub table_detections: Vec<DetectionBox>,

    // Crops (from crop nodes)
    pub formula_crops: Vec<CropRegion>,
    pub text_crops: Vec<CropRegion>,
    pub handwriting_crops: Vec<CropRegion>,

    // Table structures (grid cells from table structure recognition, with absolute coords)
    pub table_structures: Vec<RecognizedTable>,

    // Blocks (from recognizer nodes)
    pub formula_blocks: Vec<Block>,
    pub text_blocks: Vec<Block>,
    pub handwriting_blocks: Vec<Block>,
    pub table_blocks: Vec<Block>,

    // Page-level results (for multi-page)
    pub page_results: Vec<Vec<Block>>,

    // Region graph (from region_resolve_node — P4 migration)
    pub region_candidates: Vec<RegionCandidate>,
    pub resolved_regions: Vec<ResolvedRegion>,

    // Routing: what each recognizer should process
    pub recognition_targets: Vec<RecognitionTarget>,
}

/// A recognized table with its bounding box and grid cells.
/// Cells use absolute page coordinates (not relative to the table rect).
#[derive(Debug, Clone)]
pub struct RecognizedTable {
    pub table_rect: Rect,
    pub cells: Vec<GridCell>,
}

impl RecognizedTable {
    pub fn new(table_rect: Rect) -> Self {
        Self {
            table_rect,
            cells: Vec::new(),
        }
    }
}

/// A cropped region with its bounding box and image data.
#[derive(Debug, Clone)]
pub struct CropRegion {
    pub rect: Rect,
    pub image: SnipperImage,
}

impl PipelineArtifacts {
    /// Deterministic runtime lineage for debug bundles and evaluation evidence.
    pub fn debug_trace(&self) -> ArtifactTrace {
        self.artifact_graph.trace()
    }

    /// Get all blocks from all sources.
    pub fn all_blocks(&self) -> Vec<Block> {
        let mut blocks = Vec::new();
        blocks.extend(self.formula_blocks.clone());
        blocks.extend(self.text_blocks.clone());
        blocks.extend(self.handwriting_blocks.clone());
        blocks.extend(self.table_blocks.clone());
        blocks
    }

    /// Check if there are any detections.
    pub fn has_detections(&self) -> bool {
        !self.formula_detections.is_empty()
            || !self.text_detections.is_empty()
            || !self.handwriting_detections.is_empty()
            || !self.table_detections.is_empty()
    }

    /// Check if there are any blocks.
    pub fn has_blocks(&self) -> bool {
        !self.formula_blocks.is_empty()
            || !self.text_blocks.is_empty()
            || !self.handwriting_blocks.is_empty()
            || !self.table_blocks.is_empty()
    }

    /// Get total detection count.
    pub fn detection_count(&self) -> usize {
        self.formula_detections.len()
            + self.text_detections.len()
            + self.handwriting_detections.len()
            + self.table_detections.len()
    }

    /// Get total block count.
    pub fn block_count(&self) -> usize {
        self.formula_blocks.len()
            + self.text_blocks.len()
            + self.handwriting_blocks.len()
            + self.table_blocks.len()
    }
}
