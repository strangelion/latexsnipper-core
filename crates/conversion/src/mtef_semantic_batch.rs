//! Bounded in-call reuse of the experimental MTEF semantic profile.
//! Equality is raw-byte equality, never a shape key or normalized TeX match.
//! Each occurrence retains its own source pointer; host IDs remain independent.

use std::collections::HashMap;
use std::fmt;

use crate::mtef_readonly::FieldValue;
use crate::mtef_semantic::{read_mtef_v5_counted, SemanticRead};

pub const MAX_BATCH_ITEMS: usize = 10_000;
pub const MAX_BATCH_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BATCH_RECORDS: usize = 65_536;
pub const MAX_BATCH_ARRAY_ITEMS: usize = 65_536;
pub const MAX_BATCH_AST_WORK: usize = 32_768;
pub const MAX_BATCH_LOSSES: usize = 65_536;
pub const MAX_BATCH_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticBatchLimit {
    Items,
    InputBytes,
    Records,
    ArrayItems,
    AstWork,
    Losses,
    OutputBytes,
}

impl fmt::Display for SemanticBatchLimit {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "MTEF semantic batch {:?} budget exceeded", self)
    }
}
impl std::error::Error for SemanticBatchLimit {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchStats {
    pub occurrences: usize,
    pub unique_reads: usize,
    pub reused_reads: usize,
    /// Including duplicates: bounds all hash/equality input work.
    pub input_bytes: usize,
    /// Unique reports only, including failed source inspections.
    pub inspected_records: usize,
    pub structured_array_items: usize,
    pub ast_work: usize,
    pub losses: usize,
    pub output_bytes: usize,
}

#[derive(Debug)]
pub struct SemanticBatch<'source> {
    sources: Vec<&'source [u8]>,
    indices: Vec<usize>,
    reports: Vec<SemanticRead<'source>>,
    stats: BatchStats,
}

pub struct SemanticBatchEntry<'batch, 'source> {
    pub source: &'source [u8],
    /// Borrowed report for the first identical source. Its input pointer is not
    /// a host object identity; `source` is this occurrence's original input.
    pub report: &'batch SemanticRead<'source>,
}

impl<'source> SemanticBatch<'source> {
    pub fn len(&self) -> usize {
        self.sources.len()
    }
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
    pub fn stats(&self) -> BatchStats {
        self.stats
    }
    pub fn get(&self, index: usize) -> Option<SemanticBatchEntry<'_, 'source>> {
        Some(SemanticBatchEntry {
            source: self.sources.get(index)?,
            report: self.reports.get(*self.indices.get(index)?)?,
        })
    }
}

/// Read each distinct input once. Hash-map collisions require full equality.
/// Semantic source failures remain per-entry errors; a resource-budget error
/// returns no partial batch. Callers retain inputs and can split bounded chunks.
/// No process-global cache, disk write, AST clone or third-party activation.
pub fn read_mtef_v5_batch<'source>(
    sources: &[&'source [u8]],
) -> Result<SemanticBatch<'source>, SemanticBatchLimit> {
    if sources.len() > MAX_BATCH_ITEMS {
        return Err(SemanticBatchLimit::Items);
    }
    let mut stats = BatchStats {
        occurrences: sources.len(),
        ..BatchStats::default()
    };
    for source in sources {
        stats.input_bytes = add(
            stats.input_bytes,
            source.len(),
            MAX_BATCH_INPUT_BYTES,
            SemanticBatchLimit::InputBytes,
        )?;
    }
    let mut batch = SemanticBatch {
        sources: sources.to_vec(),
        indices: Vec::with_capacity(sources.len()),
        reports: Vec::new(),
        stats,
    };
    let mut known = HashMap::<&[u8], usize>::new();
    for source in sources {
        if let Some(&index) = known.get(source) {
            batch.indices.push(index);
            batch.stats.reused_reads += 1;
            continue;
        }
        let (report, work) = read_mtef_v5_counted(source);
        batch.stats.inspected_records = add(
            batch.stats.inspected_records,
            report.inspection.records.len(),
            MAX_BATCH_RECORDS,
            SemanticBatchLimit::Records,
        )?;
        // A small record count can still contain many preference dimensions,
        // styles or ruler entries. Bound their stored metadata independently.
        for record in &report.inspection.records {
            for field in &record.fields {
                let count = match &field.value {
                    FieldValue::Dimensions(values) => values.len(),
                    FieldValue::Styles(values) => values.len(),
                    FieldValue::Tabs(values) => values.len(),
                    _ => 0,
                };
                batch.stats.structured_array_items = add(
                    batch.stats.structured_array_items,
                    count,
                    MAX_BATCH_ARRAY_ITEMS,
                    SemanticBatchLimit::ArrayItems,
                )?;
            }
        }
        batch.stats.ast_work = add(
            batch.stats.ast_work,
            work,
            MAX_BATCH_AST_WORK,
            SemanticBatchLimit::AstWork,
        )?;
        batch.stats.losses = add(
            batch.stats.losses,
            report.losses.len(),
            MAX_BATCH_LOSSES,
            SemanticBatchLimit::Losses,
        )?;
        batch.stats.output_bytes = add(
            batch.stats.output_bytes,
            report.latex.as_ref().map_or(0, String::len),
            MAX_BATCH_OUTPUT_BYTES,
            SemanticBatchLimit::OutputBytes,
        )?;
        let index = batch.reports.len();
        batch.reports.push(report);
        batch.indices.push(index);
        batch.stats.unique_reads += 1;
        known.insert(source, index);
    }
    Ok(batch)
}

fn add(
    current: usize,
    amount: usize,
    maximum: usize,
    error: SemanticBatchLimit,
) -> Result<usize, SemanticBatchLimit> {
    current
        .checked_add(amount)
        .filter(|total| *total <= maximum)
        .ok_or(error)
}
