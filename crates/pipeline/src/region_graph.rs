//! Unified region graph for conflict resolution.
//!
//! Collects all detection candidates (text, formula, table, figure, etc.)
//! into a unified graph, resolves ownership conflicts, and determines
//! reading order.

use latexsnipper_ast::{Quad, Rect};
use latexsnipper_inference::DetectionBox;

use crate::artifacts::RecognizedTable;

/// Unique identifier for a region.
pub type RegionId = usize;

/// Kind of content a region represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    TextLine,
    TextParagraph,
    FormulaInline,
    FormulaDisplay,
    Table,
    TableCell,
    Figure,
    Caption,
    Heading,
    Header,
    Footer,
    Unknown,
}

/// Which model/pipeline stage produced this candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionProducer {
    TextDetector,
    FormulaDetector,
    TableDetector,
    HandwritingDetector,
    LayoutAnalysis,
}

/// Reference back into the original artifact vector — enables correct
/// legacy projection without assuming array index ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactRef {
    FormulaDetection(usize),
    TextDetection(usize),
    TableDetection(usize),
    HandwritingDetection(usize),
    LayoutRegion(usize),
    TableCell { table: usize, cell: usize },
}

/// A candidate region before conflict resolution.
#[derive(Debug, Clone)]
pub struct RegionCandidate {
    pub id: RegionId,
    pub kind: RegionKind,
    pub rect: Rect,
    pub quad: Option<Quad>,
    pub confidence: f32,
    pub producer: RegionProducer,
    pub page: usize,
    /// Reference back to the source artifact vector/index.
    pub artifact_ref: ArtifactRef,
}

/// Explicit routing target produced by RegionResolveNode.
/// Tells each recognizer exactly which regions it should process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognitionTarget {
    /// Top-level text region, index into text_detections.
    TopLevelText { detection_index: usize },
    /// Top-level formula region, index into formula_detections.
    TopLevelFormula { detection_index: usize },
    /// Top-level handwriting region, index into handwriting_detections.
    TopLevelHandwriting { detection_index: usize },
    /// Table cell that needs text recognition: (table_index, cell_index).
    TableCell { table_index: usize, cell_index: usize },
}

/// Resolved ownership of a region after conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionOwner {
    /// Standalone top-level region (text paragraph, formula, etc.)
    Independent,
    /// Owned by a parent (e.g., TableCell → Table)
    Child(RegionId),
    /// Superseded / discarded (overlapping higher-priority region)
    Discarded,
}

/// A region after conflict resolution, with ownership and reading order.
#[derive(Debug, Clone)]
pub struct ResolvedRegion {
    pub candidate: RegionCandidate,
    pub owner: RegionOwner,
    pub parent: Option<RegionId>,
    pub children: Vec<RegionId>,
    pub reading_order: Option<usize>,
}

// ── priority ordering for conflict resolution ─────────────────────────

/// Priority ranking: higher = wins conflicts.
#[allow(dead_code)]
fn kind_priority(kind: RegionKind) -> u8 {
    match kind {
        RegionKind::Table => 100,
        RegionKind::Figure => 90,
        RegionKind::FormulaDisplay => 80,
        RegionKind::FormulaInline => 70,
        RegionKind::Heading => 60,
        RegionKind::Caption => 50,
        RegionKind::TextParagraph => 40,
        RegionKind::TextLine => 30,
        RegionKind::Header => 20,
        RegionKind::Footer => 10,
        RegionKind::Unknown => 0,
        RegionKind::TableCell => 0, // resolved by parent Table
    }
}

// ── IoU helper ────────────────────────────────────────────────────────

fn iou(a: &Rect, b: &Rect) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = a.right().min(b.right());
    let y2 = a.bottom().min(b.bottom());
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a.width * a.height + b.width * b.height - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn contains_ratio(outer: &Rect, inner: &Rect) -> f32 {
    let x1 = outer.x.max(inner.x);
    let y1 = outer.y.max(inner.y);
    let x2 = outer.right().min(inner.right());
    let y2 = outer.bottom().min(inner.bottom());
    let overlap = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let inner_area = inner.width * inner.height;
    if inner_area <= 0.0 {
        0.0
    } else {
        overlap / inner_area
    }
}

// ── RegionGraph ───────────────────────────────────────────────────────

/// Unified region graph for collecting, resolving, and ordering regions.
pub struct RegionGraph {
    candidates: Vec<RegionCandidate>,
    next_id: RegionId,
}

impl RegionGraph {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a pre-existing candidate (from layout analysis or other prior stage).
    pub fn add_candidate(&mut self, candidate: RegionCandidate) {
        let id = self.next_id;
        self.next_id += 1;
        self.candidates.push(RegionCandidate { id, ..candidate });
    }

    /// Add a detection with a given kind.
    pub fn add_detection(
        &mut self,
        det: &DetectionBox,
        kind: RegionKind,
        producer: RegionProducer,
        page: usize,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let artifact_ref = match producer {
            RegionProducer::FormulaDetector => ArtifactRef::FormulaDetection(0),
            RegionProducer::TextDetector => ArtifactRef::TextDetection(0),
            RegionProducer::TableDetector => ArtifactRef::TableDetection(0),
            RegionProducer::HandwritingDetector => ArtifactRef::HandwritingDetection(0),
            RegionProducer::LayoutAnalysis => ArtifactRef::LayoutRegion(0),
        };
        self.candidates.push(RegionCandidate {
            id,
            kind,
            rect: det.rect,
            quad: det.quad,
            confidence: det.confidence,
            producer,
            page,
            artifact_ref,
        });
    }

    /// Add a table structure (produces a Table + TableCell regions).
    pub fn add_table(&mut self, table: &RecognizedTable, page: usize) -> RegionId {
        let table_id = self.next_id;
        self.next_id += 1;
        self.candidates.push(RegionCandidate {
            id: table_id,
            kind: RegionKind::Table,
            rect: table.table_rect,
            quad: None,
            confidence: 1.0,
            producer: RegionProducer::TableDetector,
            page,
            artifact_ref: ArtifactRef::LayoutRegion(0),
        });

        for (cell_idx, cell) in table.cells.iter().enumerate() {
            let cell_id = self.next_id;
            self.next_id += 1;
            self.candidates.push(RegionCandidate {
                id: cell_id,
                kind: RegionKind::TableCell,
                rect: cell.rect,
                quad: None,
                confidence: 1.0,
                producer: RegionProducer::TableDetector,
                page,
                artifact_ref: ArtifactRef::TableCell {
                    table: table_id,
                    cell: cell_idx,
                },
            });
        }

        table_id
    }

    /// Get a reference to all candidates.
    pub fn candidates(&self) -> impl Iterator<Item = &RegionCandidate> {
        self.candidates.iter()
    }

    /// Resolve conflicts and return resolved regions with reading order.
    pub fn resolve(&self) -> Vec<ResolvedRegion> {
        let mut resolved: Vec<ResolvedRegion> = self
            .candidates
            .iter()
            .map(|c| ResolvedRegion {
                candidate: c.clone(),
                owner: RegionOwner::Independent,
                parent: None,
                children: Vec::new(),
                reading_order: None,
            })
            .collect();

        // ── Phase 1: Table contains Text/Formula → assign as children ──
        // Collect relationships first to avoid borrow conflicts
        let mut child_assignments: Vec<(usize, usize)> = Vec::new(); // (child_idx, parent_candidate_id)
        let table_ids: Vec<usize> = resolved
            .iter()
            .enumerate()
            .filter(|(_, r)| r.candidate.kind == RegionKind::Table)
            .map(|(i, _)| i)
            .collect();

        let cell_ids: Vec<usize> = resolved
            .iter()
            .enumerate()
            .filter(|(_, r)| r.candidate.kind == RegionKind::TableCell)
            .map(|(i, _)| i)
            .collect();

        for i in 0..resolved.len() {
            let kind = resolved[i].candidate.kind;
            if kind == RegionKind::TextLine
                || kind == RegionKind::TextParagraph
                || kind == RegionKind::FormulaInline
                || kind == RegionKind::FormulaDisplay
            {
                let rect = resolved[i].candidate.rect;
                // Check if contained in any table cell
                for &cell_idx in &cell_ids {
                    let cell_rect = resolved[cell_idx].candidate.rect;
                    if contains_ratio(&cell_rect, &rect) > 0.5 {
                        child_assignments.push((i, resolved[cell_idx].candidate.id));
                        break;
                    }
                }
                // If not in a cell, check if inside table rect
                if child_assignments.last().is_none_or(|&(ci, _)| ci != i) {
                    for &tbl_idx in &table_ids {
                        let tbl_rect = resolved[tbl_idx].candidate.rect;
                        if contains_ratio(&tbl_rect, &rect) > 0.5 {
                            child_assignments.push((i, resolved[tbl_idx].candidate.id));
                            break;
                        }
                    }
                }
            }
        }
        // Apply child assignments
        for &(child_idx, parent_id) in &child_assignments {
            resolved[child_idx].owner = RegionOwner::Child(parent_id);
        }
        for &(child_idx, parent_id) in &child_assignments {
            let child_candidate_id = resolved[child_idx].candidate.id;
            if let Some(parent) = resolved.iter_mut().find(|r| r.candidate.id == parent_id) {
                parent.children.push(child_candidate_id);
            }
        }

        // ── Phase 2: Formula vs Text overlap → Formula wins ──
        for i in 0..resolved.len() {
            if resolved[i].owner != RegionOwner::Independent {
                continue;
            }
            let kind_i = resolved[i].candidate.kind;
            if kind_i != RegionKind::FormulaDisplay && kind_i != RegionKind::FormulaInline {
                continue;
            }

            for j in 0..resolved.len() {
                if i == j || resolved[j].owner != RegionOwner::Independent {
                    continue;
                }
                let kind_j = resolved[j].candidate.kind;
                if kind_j != RegionKind::TextLine && kind_j != RegionKind::TextParagraph {
                    continue;
                }

                let overlap = iou(&resolved[i].candidate.rect, &resolved[j].candidate.rect);
                if overlap > 0.3 {
                    // Text overlaps with formula — discard the text portion
                    resolved[j].owner = RegionOwner::Discarded;
                }
            }
        }

        // ── Phase 3: Assign reading order (use indices only) ──
        // Collect (index, center_y, x) for independent regions
        let mut top_level: Vec<(usize, f32, f32)> = resolved
            .iter()
            .enumerate()
            .filter(|(_, r)| r.owner == RegionOwner::Independent)
            .map(|(i, r)| (i, r.candidate.rect.center_y(), r.candidate.rect.x))
            .collect();

        // Sort: single-column y-bucket + x tie-breaker
        top_level.sort_by(|(_, ay, ax), (_, by, bx)| {
            ay.partial_cmp(by)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ax.partial_cmp(bx).unwrap_or(std::cmp::Ordering::Equal))
        });

        // Group into y-buckets (lines), then sort by x within each line
        let mut buckets: Vec<Vec<(usize, f32, f32)>> = Vec::new();
        for item in top_level {
            if let Some(last) = buckets.last() {
                let last_y = last[0].1;
                if (item.1 - last_y).abs() < 20.0 {
                    buckets.last_mut().unwrap().push(item);
                    continue;
                }
            }
            buckets.push(vec![item]);
        }
        for bucket in &mut buckets {
            bucket.sort_by(|(_, _, ax), (_, _, bx)| {
                ax.partial_cmp(bx).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Assign order numbers
        let mut order = 0;
        for bucket in &buckets {
            for &(idx, _, _) in bucket {
                resolved[idx].reading_order = Some(order);
                order += 1;
            }
        }

        resolved
    }

    /// Get all resolved regions that are top-level (not discarded, not children).
    pub fn top_level<'a>(&self, resolved: &'a [ResolvedRegion]) -> Vec<&'a ResolvedRegion> {
        resolved
            .iter()
            .filter(|r| r.owner == RegionOwner::Independent)
            .collect()
    }
}

impl Default for RegionGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_det(x: f32, y: f32, w: f32, h: f32, conf: f32) -> DetectionBox {
        DetectionBox::rect(Rect::new(x, y, w, h), conf, 0, "text".into())
    }

    #[test]
    fn test_table_contains_text_discards_text() {
        let mut graph = RegionGraph::new();
        // Table occupies top-left
        graph.add_detection(
            &make_det(0.0, 0.0, 200.0, 200.0, 1.0),
            RegionKind::Table,
            RegionProducer::TableDetector,
            0,
        );
        // Text inside table
        graph.add_detection(
            &make_det(10.0, 10.0, 50.0, 20.0, 0.9),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );
        // Text outside table
        graph.add_detection(
            &make_det(300.0, 10.0, 100.0, 20.0, 0.9),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );

        let resolved = graph.resolve();
        let inside = resolved
            .iter()
            .find(|r| r.candidate.rect.x == 10.0)
            .unwrap();
        let outside = resolved
            .iter()
            .find(|r| r.candidate.rect.x == 300.0)
            .unwrap();

        assert_eq!(
            inside.owner,
            RegionOwner::Child(1),
            "Text inside table should be child of table (id=1)"
        );
        assert_eq!(
            outside.owner,
            RegionOwner::Independent,
            "Text outside table should remain independent"
        );
    }

    #[test]
    fn test_formula_wins_over_text() {
        let mut graph = RegionGraph::new();
        // Formula
        graph.add_detection(
            &make_det(10.0, 10.0, 100.0, 30.0, 0.95),
            RegionKind::FormulaDisplay,
            RegionProducer::FormulaDetector,
            0,
        );
        // Text overlapping with formula
        graph.add_detection(
            &make_det(10.0, 10.0, 100.0, 30.0, 0.8),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );

        let resolved = graph.resolve();
        let formula = resolved
            .iter()
            .find(|r| r.candidate.kind == RegionKind::FormulaDisplay)
            .unwrap();
        let text = resolved
            .iter()
            .find(|r| r.candidate.kind == RegionKind::TextLine)
            .unwrap();

        assert_eq!(formula.owner, RegionOwner::Independent);
        assert_eq!(text.owner, RegionOwner::Discarded);
    }

    #[test]
    fn test_reading_order_y_bucket() {
        let mut graph = RegionGraph::new();
        graph.add_detection(
            &make_det(200.0, 10.0, 50.0, 20.0, 0.9),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );
        graph.add_detection(
            &make_det(10.0, 10.0, 50.0, 20.0, 0.9),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );
        graph.add_detection(
            &make_det(10.0, 100.0, 50.0, 20.0, 0.9),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );

        let resolved = graph.resolve();
        let mut orders: Vec<(usize, f32)> = resolved
            .iter()
            .filter(|r| r.reading_order.is_some())
            .map(|r| (r.reading_order.unwrap(), r.candidate.rect.x))
            .collect();
        orders.sort_by_key(|(order, _)| *order);

        // Same line: x=10 before x=200; then next line
        assert_eq!(orders.len(), 3);
        assert_eq!(orders[0].1, 10.0);
        assert_eq!(orders[1].1, 200.0);
    }
}
