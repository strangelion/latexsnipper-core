//! Bounded deduplication and structural grouping for raw v5 byte inspection.
//! Structural keys classify record layouts, never mathematical equivalence.
//! No semantic converter, OLE extraction or MathType writer is implemented here.

use std::collections::HashMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::mtef_readonly::{inspect_mtef_v5, FieldValue, Inspection};

pub const MAX_BATCH_ITEMS: usize = 10_000;
pub const MAX_BATCH_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BATCH_RECORDS: usize = 65_536;
pub const STRUCTURE_KEY_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchLimit {
    Items,
    InputBytes,
    Records,
}

impl fmt::Display for BatchLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MTEF batch {:?} budget exceeded", self)
    }
}

impl std::error::Error for BatchLimit {}

#[derive(Debug)]
struct UniqueInspection<'a> {
    inspection: Inspection<'a>,
    structure_key: Option<String>,
}

/// Per-occurrence source identity is preserved even when inspection is shared.
pub struct BatchEntry<'batch, 'source> {
    pub source: &'source [u8],
    /// This report borrows the first identical input. Its spans also apply to
    /// `source`, but its source pointer must not be used as a host object ID.
    pub inspection: &'batch Inspection<'source>,
    pub structure_key: Option<&'batch str>,
}

#[derive(Debug)]
pub struct BatchInspection<'a> {
    sources: Vec<&'a [u8]>,
    inspection_indices: Vec<usize>,
    unique: Vec<UniqueInspection<'a>>,
    inspected_records: usize,
}

impl<'a> BatchInspection<'a> {
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn inspections(&self) -> usize {
        self.unique.len()
    }

    pub fn reused_inspections(&self) -> usize {
        self.len() - self.inspections()
    }

    pub fn inspected_records(&self) -> usize {
        self.inspected_records
    }

    pub fn get(&self, index: usize) -> Option<BatchEntry<'_, 'a>> {
        let source = *self.sources.get(index)?;
        let unique = &self.unique[self.inspection_indices[index]];
        Some(BatchEntry {
            source,
            inspection: &unique.inspection,
            structure_key: unique.structure_key.as_deref(),
        })
    }
}

/// Identical raw bytes are inspected once within this call. Equality is checked
/// by the map, so a hash collision cannot substitute a different byte stream.
/// Duplicates count against the input budget, bounding total hashing work.
pub fn inspect_mtef_v5_batch<'a>(sources: &[&'a [u8]]) -> Result<BatchInspection<'a>, BatchLimit> {
    if sources.len() > MAX_BATCH_ITEMS {
        return Err(BatchLimit::Items);
    }
    let mut input_bytes = 0usize;
    for source in sources {
        input_bytes = input_bytes
            .checked_add(source.len())
            .filter(|bytes| *bytes <= MAX_BATCH_INPUT_BYTES)
            .ok_or(BatchLimit::InputBytes)?;
    }

    let mut indices = HashMap::<&[u8], usize>::new();
    let mut batch = BatchInspection {
        sources: sources.to_vec(),
        inspection_indices: Vec::with_capacity(sources.len()),
        unique: Vec::new(),
        inspected_records: 0,
    };
    for source in sources {
        if let Some(index) = indices.get(source) {
            batch.inspection_indices.push(*index);
            continue;
        }
        let inspection = inspect_mtef_v5(source);
        batch.inspected_records = batch
            .inspected_records
            .checked_add(inspection.records.len())
            .filter(|records| *records <= MAX_BATCH_RECORDS)
            .ok_or(BatchLimit::Records)?;
        let structure_key = structure_key(&inspection);
        let index = batch.unique.len();
        batch.unique.push(UniqueInspection {
            inspection,
            structure_key,
        });
        indices.insert(source, index);
        batch.inspection_indices.push(index);
    }
    Ok(batch)
}

fn structure_key(inspection: &Inspection<'_>) -> Option<String> {
    // Opaque, stopped, future and empty streams cannot establish a known layout.
    if !inspection.complete
        || !inspection.diagnostics.is_empty()
        || !inspection
            .records
            .iter()
            .any(|record| (1..=5).contains(&record.record_type))
    {
        return None;
    }
    let header = inspection.header.as_ref()?;
    let mut digest = Sha256::new();
    digest.update(b"latexsnipper-mtef-record-shape");
    digest.update([
        STRUCTURE_KEY_VERSION,
        header.platform,
        header.product,
        header.product_version,
        header.product_subversion,
        u8::from(header.inline),
    ]);
    for record in &inspection.records {
        digest.update([record.record_type]);
        digest.update((record.depth as u64).to_le_bytes());
        for field in &record.fields {
            if matches!(
                field.name,
                "options" | "selector" | "variation" | "template_options" | "rows" | "columns"
            ) {
                if let FieldValue::Unsigned(value) = field.value {
                    digest.update((field.name.len() as u64).to_le_bytes());
                    digest.update(field.name.as_bytes());
                    digest.update(value.to_le_bytes());
                }
            }
        }
        digest.update([0xff]);
    }
    Some(format!(
        "mtef-shape-v{STRUCTURE_KEY_VERSION}:{:x}",
        digest.finalize()
    ))
}
