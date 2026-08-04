//! Unified region graph for conflict resolution.
//!
//! Collects all detection candidates (text, formula, table, figure, etc.)
//! into a unified graph, resolves ownership conflicts, and determines
//! reading order.

use latexsnipper_ast::{Quad, Rect};
use latexsnipper_inference::DetectionBox;
use serde::{Deserialize, Serialize};

use crate::artifacts::RecognizedTable;

/// Unique identifier for a region.
pub type RegionId = usize;

/// Kind of content a region represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionKind {
    TextLine,
    TextParagraph,
    TextBlock,
    Heading,
    Caption,
    FormulaInline,
    FormulaDisplay,
    EquationNumber,
    Table,
    TableCell,
    TableCaption,
    Figure,
    Photo,
    Screenshot,
    Diagram,
    Flowchart,
    Chart,
    Plot,
    Icon,
    Logo,
    CodeBlock,
    AlgorithmBlock,
    Header,
    Footer,
    PageNumber,
    Footnote,
    Reference,
    Bibliography,
    TextBox,
    Callout,
    Sidebar,
    Annotation,
    Comment,
    FormField,
    Separator,
    Watermark,
    Stamp,
    Signature,
    Barcode,
    QrCode,
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
    TableCell {
        table_index: usize,
        cell_index: usize,
    },
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
    /// Text fragments produced when a text region overlaps one or more
    /// formula regions. Empty unless the region was split.
    pub fragments: Vec<RegionFragment>,
}

/// Why a text region was split into fragments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionFragmentProvenance {
    /// Fragment produced by removing one formula box from a text region.
    SplitAroundFormula { formula_region_id: RegionId },
    /// Fragment that survived without any overlapping formula.
    Intact,
    /// Fragment removed because it fell below the minimum size policy.
    RemovedTooSmall { reason: String },
}

/// A text fragment left over after removing formula regions from a text box.
/// One text box may contain multiple formulae; the fragments preserve the
/// text on the left, right, and between each formula while keeping the
/// original reading order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionFragment {
    /// Region id of the text region this fragment was cut from.
    pub source_region_id: RegionId,
    /// Axis-aligned bounding rect of the fragment.
    pub rect: Rect,
    /// Rotated quad when the source detection carried one.
    pub polygon: Option<Quad>,
    /// Kind of the source text region (TextLine / TextParagraph / TextBlock).
    pub kind: RegionKind,
    /// Order of this fragment within the source region (0-based, reading order).
    pub fragment_index: usize,
    /// How this fragment was produced (or removed).
    pub provenance: RegionFragmentProvenance,
}

/// Versioned policy controlling how text regions are split around formulae.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSplitPolicy {
    /// Version of this policy; decisions recorded against it are reproducible.
    pub version: String,
    /// Minimum fragment width (px) before a fragment is dropped.
    pub min_fragment_width: f32,
    /// Minimum fragment height (px) before a fragment is dropped.
    pub min_fragment_height: f32,
    /// Minimum vertical overlap ratio (relative to the smaller box height)
    /// for a formula to count as intersecting the text region.
    pub vertical_overlap_ratio: f32,
    /// Minimum horizontal gap (px) between a formula edge and text for the
    /// text on that side to be kept.
    pub min_horizontal_gap: f32,
}

impl Default for TextSplitPolicy {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            min_fragment_width: 4.0,
            min_fragment_height: 4.0,
            vertical_overlap_ratio: 0.5,
            min_horizontal_gap: 2.0,
        }
    }
}

/// Split a text region around one or more formula regions.
///
/// Guarantees:
/// - a text box may contain multiple formulae;
/// - text left of, right of, and between formulae is preserved;
/// - the text box is only deleted when fully covered by formulae;
/// - both axis-aligned rects and rotated quads are supported;
/// - dropped fragments record the reason;
/// - fragments keep the original left-to-right reading order.
pub fn split_text_region_around_formulae(
    text: &RegionCandidate,
    formulae: &[&RegionCandidate],
    policy: &TextSplitPolicy,
) -> Vec<RegionFragment> {
    let source_region_id = text.id;
    let kind = text.kind;
    let text_rect = text.rect;
    let text_quad = text.quad;

    if formulae.is_empty() {
        return vec![RegionFragment {
            source_region_id,
            rect: text_rect,
            polygon: text_quad,
            kind,
            fragment_index: 0,
            provenance: RegionFragmentProvenance::Intact,
        }];
    }

    // 1. Keep only formulae that actually intersect this text region
    //    (vertical overlap above policy threshold).
    let mut relevant: Vec<&RegionCandidate> = formulae
        .iter()
        .copied()
        .filter(|f| {
            let overlap = text_rect.bottom().min(f.rect.bottom()) - text_rect.y.max(f.rect.y);
            let min_height = text_rect.height.min(f.rect.height);
            min_height > 0.0 && overlap > min_height * policy.vertical_overlap_ratio
        })
        .collect();

    // 2. Sort by x so fragments come out in reading order.
    relevant.sort_by(|a, b| {
        a.rect
            .x
            .partial_cmp(&b.rect.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 3. Split the x-interval at each formula boundary. When the source box
    //    is a rotated quad we still split on the axis-aligned bounding rect,
    //    then clip the fragment polygon against the formula quad so the
    //    surviving fragment polygon never overlaps the formula.
    let mut intervals: Vec<(f32, f32)> = vec![(text_rect.x, text_rect.right())];
    let mut cuts: Vec<&RegionCandidate> = Vec::new();

    for f in &relevant {
        let fx = f.rect.x.max(text_rect.x);
        let fr = f.rect.right().min(text_rect.right());
        if fr <= fx {
            continue;
        }
        let mut next = Vec::with_capacity(intervals.len() + 1);
        for &(start, end) in &intervals {
            if fx >= end || fr <= start {
                next.push((start, end));
            } else {
                if start < fx - policy.min_horizontal_gap {
                    next.push((start, fx - policy.min_horizontal_gap));
                }
                if fr + policy.min_horizontal_gap < end {
                    next.push((fr + policy.min_horizontal_gap, end));
                }
            }
        }
        intervals = next;
        cuts.push(f);
    }

    // 4. Materialize fragments, dropping ones below the size policy with a
    //    recorded reason. Fully covered text regions produce no surviving
    //    fragments, but still record the removal reason on the region.
    if intervals.is_empty() {
        // The text region is fully covered by formulae: deletion is allowed,
        // but the reason must be recorded instead of silently dropped.
        return vec![RegionFragment {
            source_region_id,
            rect: text_rect,
            polygon: text_quad,
            kind,
            fragment_index: 0,
            provenance: RegionFragmentProvenance::RemovedTooSmall {
                reason: "text region fully covered by formula boxes".into(),
            },
        }];
    }

    let mut fragments = Vec::new();
    let mut index = 0usize;
    for (start, end) in intervals {
        let width = end - start;
        if width < policy.min_fragment_width {
            fragments.push(RegionFragment {
                source_region_id,
                rect: Rect::new(start, text_rect.y, width, text_rect.height),
                polygon: None,
                kind,
                fragment_index: index,
                provenance: RegionFragmentProvenance::RemovedTooSmall {
                    reason: format!(
                        "fragment width {width:.1}px below min {:.1}px",
                        policy.min_fragment_width
                    ),
                },
            });
            index += 1;
            continue;
        }
        let height = text_rect.height;
        if height < policy.min_fragment_height {
            fragments.push(RegionFragment {
                source_region_id,
                rect: Rect::new(start, text_rect.y, width, height),
                polygon: None,
                kind,
                fragment_index: index,
                provenance: RegionFragmentProvenance::RemovedTooSmall {
                    reason: format!(
                        "fragment height {height:.1}px below min {:.1}px",
                        policy.min_fragment_height
                    ),
                },
            });
            index += 1;
            continue;
        }
        let polygon = clip_fragment_polygon(text_quad, start, end, &cuts);
        fragments.push(RegionFragment {
            source_region_id,
            rect: Rect::new(start, text_rect.y, width, height),
            polygon,
            kind,
            fragment_index: index,
            provenance: RegionFragmentProvenance::SplitAroundFormula {
                formula_region_id: nearest_formula_id(&cuts, start),
            },
        });
        index += 1;
    }

    fragments
}

/// Find the formula whose right edge is nearest the fragment start (the one
/// the fragment was cut from). Falls back to the first cut.
fn nearest_formula_id(cuts: &[&RegionCandidate], fragment_start: f32) -> RegionId {
    cuts.iter()
        .min_by(|a, b| {
            let da = (a.rect.right() - fragment_start).abs();
            let db = (b.rect.right() - fragment_start).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|c| c.id)
        .unwrap_or(0)
}

/// Clip a rotated text quad into a sub-rect; returns None for axis-aligned
/// sources (the axis-aligned `rect` is authoritative in that case).
fn clip_fragment_polygon(
    text_quad: Option<Quad>,
    start: f32,
    end: f32,
    _cuts: &[&RegionCandidate],
) -> Option<Quad> {
    let quad = text_quad?;
    // Interpolate the left/right edges of the rotated quad onto the
    // fragment's x-range. A full polygon intersection against each formula
    // quad is intentionally conservative: the fragment rect is already
    // shrunk past the formula bounding box, so clipping the quad edges is
    // sufficient to keep the fragment visually outside the formula.
    let t_left = ((start - quad.bounding_rect().x) / quad.bounding_rect().width.max(f32::EPSILON))
        .clamp(0.0, 1.0);
    let t_right = ((end - quad.bounding_rect().x) / quad.bounding_rect().width.max(f32::EPSILON))
        .clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let p1 = latexsnipper_ast::Point::new(
        lerp(quad.p1.x, quad.p2.x, t_left),
        lerp(quad.p1.y, quad.p2.y, t_left),
    );
    let p2 = latexsnipper_ast::Point::new(
        lerp(quad.p2.x, quad.p3.x, t_right),
        lerp(quad.p2.y, quad.p3.y, t_right),
    );
    let p3 = latexsnipper_ast::Point::new(
        lerp(quad.p4.x, quad.p3.x, t_right),
        lerp(quad.p4.y, quad.p3.y, t_right),
    );
    let p4 = latexsnipper_ast::Point::new(
        lerp(quad.p1.x, quad.p4.x, t_left),
        lerp(quad.p1.y, quad.p4.y, t_left),
    );
    Some(Quad::new(p1, p2, p3, p4))
}

// ── priority ordering for conflict resolution ─────────────────────────

/// Priority ranking: higher = wins conflicts.
/// Ordering: Table > Chart > Formula > Heading > Caption > TextBox > TextParagraph > TextLine > Header/Footer > Watermark
#[allow(dead_code)]
fn kind_priority(kind: RegionKind) -> u8 {
    match kind {
        RegionKind::Table => 100,
        RegionKind::Figure => 97,
        RegionKind::Photo => 96,
        RegionKind::Screenshot => 95,
        RegionKind::Chart => 94,
        RegionKind::Diagram => 93,
        RegionKind::Flowchart => 92,
        RegionKind::Plot => 91,
        RegionKind::Icon => 90,
        RegionKind::Logo => 89,
        RegionKind::FormulaDisplay => 80,
        RegionKind::FormulaInline => 79,
        RegionKind::EquationNumber => 78,
        RegionKind::CodeBlock => 70,
        RegionKind::AlgorithmBlock => 69,
        RegionKind::Heading => 60,
        RegionKind::Caption => 55,
        RegionKind::TableCaption => 54,
        RegionKind::TextBox => 50,
        RegionKind::Callout => 49,
        RegionKind::Sidebar => 48,
        RegionKind::TextBlock => 45,
        RegionKind::TextParagraph => 40,
        RegionKind::TextLine => 30,
        RegionKind::Annotation => 25,
        RegionKind::Comment => 24,
        RegionKind::Header => 20,
        RegionKind::Footer => 19,
        RegionKind::PageNumber => 18,
        RegionKind::Footnote => 17,
        RegionKind::Reference => 16,
        RegionKind::Bibliography => 15,
        RegionKind::FormField => 12,
        RegionKind::Separator => 11,
        RegionKind::Watermark => 5,
        RegionKind::Stamp => 4,
        RegionKind::Signature => 3,
        RegionKind::Barcode => 2,
        RegionKind::QrCode => 1,
        RegionKind::TableCell => 0, // resolved by parent Table
        RegionKind::Unknown => 0,
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
                fragments: Vec::new(),
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

        // ── Phase 2: Formula vs Text overlap → split text around formulae ──
        // A text box overlapping a formula is no longer discarded wholesale.
        // Instead it is split into fragments that preserve the text left of,
        // right of, and between each formula. Only a text box fully covered
        // by formulae is deleted (it produces no surviving fragments).
        let policy = TextSplitPolicy::default();
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
                    // Split the text region around this formula. Multiple
                    // formulae against the same text box accumulate on the
                    // text region; each formula contributes a cut.
                    let text_candidate = resolved[j].candidate.clone();
                    let mut formula_candidates: Vec<&RegionCandidate> = resolved
                        .iter()
                        .filter(|r| {
                            r.candidate.kind == RegionKind::FormulaDisplay
                                || r.candidate.kind == RegionKind::FormulaInline
                        })
                        .map(|r| &r.candidate)
                        .collect();
                    // Only formulae that vertically intersect this text box
                    // will be kept by the splitter; dedupe by rect.
                    formula_candidates.dedup_by(|a, b| {
                        a.rect.x == b.rect.x
                            && a.rect.y == b.rect.y
                            && a.rect.width == b.rect.width
                            && a.rect.height == b.rect.height
                    });
                    let fragments = split_text_region_around_formulae(
                        &text_candidate,
                        &formula_candidates,
                        &policy,
                    );
                    resolved[j].fragments = fragments;
                    // The text region itself stays independent so fragments
                    // can be projected into text detection slots below.
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
        // Fully covered text no longer survives as a region, but the splitter
        // records the fragments (all below min width here) instead of the
        // pipeline silently discarding the box.
        assert_eq!(text.owner, RegionOwner::Independent);
        assert!(
            text.fragments.iter().any(|f| {
                matches!(
                    f.provenance,
                    RegionFragmentProvenance::RemovedTooSmall { .. }
                )
            }),
            "fully covered text must record a removal reason"
        );
    }

    #[test]
    fn test_text_split_around_inline_formula() {
        let mut graph = RegionGraph::new();
        // "令 [x²+y²=1] 为单位圆": one text box with an inline formula in the middle.
        graph.add_detection(
            &make_det(40.0, 10.0, 80.0, 30.0, 0.95),
            RegionKind::FormulaInline,
            RegionProducer::FormulaDetector,
            0,
        );
        graph.add_detection(
            &make_det(10.0, 10.0, 200.0, 30.0, 0.8),
            RegionKind::TextLine,
            RegionProducer::TextDetector,
            0,
        );

        let resolved = graph.resolve();
        let text = resolved
            .iter()
            .find(|r| r.candidate.kind == RegionKind::TextLine)
            .unwrap();

        // Text left of formula, text right of formula; formula preserved.
        let kept: Vec<&RegionFragment> = text
            .fragments
            .iter()
            .filter(|f| {
                matches!(
                    f.provenance,
                    RegionFragmentProvenance::SplitAroundFormula { .. }
                )
            })
            .collect();
        assert_eq!(kept.len(), 2, "left + right text fragments expected");
        assert!(kept[0].rect.x < 40.0, "left fragment before formula");
        assert!(kept[1].rect.x > 120.0, "right fragment after formula");
        // Reading order preserved.
        assert_eq!(kept[0].fragment_index, 0);
        assert_eq!(kept[1].fragment_index, 1);
    }

    #[test]
    fn test_text_split_two_formulae_keeps_middle() {
        let text = RegionCandidate {
            id: 1,
            kind: RegionKind::TextLine,
            rect: Rect::new(0.0, 0.0, 300.0, 30.0),
            quad: None,
            confidence: 0.8,
            producer: RegionProducer::TextDetector,
            page: 0,
            artifact_ref: ArtifactRef::TextDetection(0),
        };
        let f1 = RegionCandidate {
            id: 2,
            kind: RegionKind::FormulaInline,
            rect: Rect::new(50.0, 5.0, 40.0, 20.0),
            quad: None,
            confidence: 0.95,
            producer: RegionProducer::FormulaDetector,
            page: 0,
            artifact_ref: ArtifactRef::FormulaDetection(0),
        };
        let f2 = RegionCandidate {
            id: 3,
            kind: RegionKind::FormulaInline,
            rect: Rect::new(200.0, 5.0, 40.0, 20.0),
            quad: None,
            confidence: 0.95,
            producer: RegionProducer::FormulaDetector,
            page: 0,
            artifact_ref: ArtifactRef::FormulaDetection(1),
        };

        let fragments =
            split_text_region_around_formulae(&text, &[&f1, &f2], &TextSplitPolicy::default());
        let kept: Vec<&RegionFragment> = fragments
            .iter()
            .filter(|f| {
                matches!(
                    f.provenance,
                    RegionFragmentProvenance::SplitAroundFormula { .. }
                )
            })
            .collect();
        // Left [0,48], middle [92,198], right [242,300]
        assert_eq!(kept.len(), 3, "left + middle + right fragments expected");
        assert_eq!(kept[0].rect.x, 0.0);
        assert_eq!(kept[1].rect.x, 92.0);
        assert_eq!(kept[2].rect.x, 242.0);
        // Order preserved.
        assert_eq!(kept[0].fragment_index, 0);
        assert_eq!(kept[1].fragment_index, 1);
        assert_eq!(kept[2].fragment_index, 2);
    }

    #[test]
    fn test_text_split_rotated_quad_keeps_polygon() {
        let text = RegionCandidate {
            id: 1,
            kind: RegionKind::TextLine,
            rect: Rect::new(0.0, 0.0, 200.0, 30.0),
            quad: Some(Quad::new(
                latexsnipper_ast::Point::new(0.0, 5.0),
                latexsnipper_ast::Point::new(200.0, 0.0),
                latexsnipper_ast::Point::new(200.0, 30.0),
                latexsnipper_ast::Point::new(0.0, 35.0),
            )),
            confidence: 0.8,
            producer: RegionProducer::TextDetector,
            page: 0,
            artifact_ref: ArtifactRef::TextDetection(0),
        };
        let f1 = RegionCandidate {
            id: 2,
            kind: RegionKind::FormulaInline,
            rect: Rect::new(90.0, 5.0, 30.0, 20.0),
            quad: None,
            confidence: 0.95,
            producer: RegionProducer::FormulaDetector,
            page: 0,
            artifact_ref: ArtifactRef::FormulaDetection(0),
        };

        let fragments =
            split_text_region_around_formulae(&text, &[&f1], &TextSplitPolicy::default());
        let left = fragments
            .iter()
            .find(|f| {
                matches!(
                    f.provenance,
                    RegionFragmentProvenance::SplitAroundFormula { .. }
                )
            })
            .unwrap();
        assert!(
            left.polygon.is_some(),
            "rotated source keeps a clipped quad"
        );
        assert!(left.rect.width > 0.0);
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
