//! Versioned decoder KV-cache state contracts.
//!
//! State order is never treated as semantic identity. A decoder export must
//! provide this schema and name every input/output before incremental decoding
//! can be enabled.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DECODER_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecoderStateSchema {
    pub version: u32,
    pub model_id: String,
    pub model_version: String,
    pub decoder_sha256: String,
    pub entries: Vec<DecoderStateEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecoderStateEntry {
    pub name: String,
    pub paddle_variable: String,
    pub onnx_input: String,
    pub onnx_output: String,
    pub role: DecoderStateRole,
    pub dtype: DecoderDType,
    pub rank: usize,
    pub shape_semantics: Vec<AxisSemantic>,
    pub step0_shape: Vec<Option<usize>>,
    pub step1_shape: Vec<Option<usize>>,
    pub growth_axis: Option<usize>,
    pub layer_index: Option<usize>,
    pub attention_kind: Option<AttentionKind>,
    pub update_rule: String,
    pub encoder_static: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecoderStateRole {
    SelfKey,
    SelfValue,
    CrossKey,
    CrossValue,
    Position,
    AttentionMask,
    TokenIds,
    SequenceLengths,
    BeamScores,
    Finished,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecoderDType {
    Float16,
    Float32,
    Float64,
    Int32,
    Int64,
    Bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisSemantic {
    Batch,
    Beam,
    Head,
    Sequence,
    HeadDimension,
    Layer,
    StaticEncoderSequence,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    SelfAttention,
    CrossAttention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderStateObservation {
    pub name: String,
    pub dtype: DecoderDType,
    pub shape: Vec<usize>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecoderStateError {
    #[error("CACHE_SCHEMA_MISMATCH: unsupported schema version {0}")]
    UnsupportedVersion(u32),
    #[error("CACHE_SCHEMA_MISMATCH: model identity or decoder checksum is empty")]
    MissingIdentity,
    #[error("CACHE_SCHEMA_MISMATCH: decoder state schema has no entries")]
    EmptySchema,
    #[error("CACHE_SCHEMA_MISMATCH: duplicate state name or mapping '{0}'")]
    DuplicateMapping(String),
    #[error(
        "CACHE_SCHEMA_MISMATCH: state '{name}' has rank {rank} but {semantics} axis semantics"
    )]
    RankMismatch {
        name: String,
        rank: usize,
        semantics: usize,
    },
    #[error("CACHE_SCHEMA_MISMATCH: state '{0}' has invalid step shape rank")]
    StepShapeRank(String),
    #[error("CACHE_SCHEMA_MISMATCH: state '{0}' has an invalid growth axis")]
    InvalidGrowthAxis(String),
    #[error("CACHE_SCHEMA_MISMATCH: state '{0}' cache role does not match its attention metadata")]
    InvalidAttentionMetadata(String),
    #[error("CACHE_OUTPUT_MAPPING_MISMATCH: no schema entry maps observation '{0}'")]
    UnknownObservation(String),
    #[error("CACHE_DTYPE_MISMATCH: state '{name}' expected {expected:?}, found {actual:?}")]
    DTypeMismatch {
        name: String,
        expected: DecoderDType,
        actual: DecoderDType,
    },
    #[error("CACHE_SCHEMA_MISMATCH: state '{name}' expected rank {expected}, found {actual}")]
    ObservationRankMismatch {
        name: String,
        expected: usize,
        actual: usize,
    },
    #[error("CACHE_BATCH_MISMATCH: state '{name}' changed batch/beam axis {axis} from {before} to {after}")]
    BatchMismatch {
        name: String,
        axis: usize,
        before: usize,
        after: usize,
    },
    #[error("CACHE_SEQUENCE_NOT_MONOTONIC: state '{name}' sequence axis {axis} changed from {before} to {after}")]
    SequenceNotMonotonic {
        name: String,
        axis: usize,
        before: usize,
        after: usize,
    },
}

impl DecoderStateSchema {
    pub fn validate(&self) -> Result<(), DecoderStateError> {
        if self.version != DECODER_STATE_SCHEMA_VERSION {
            return Err(DecoderStateError::UnsupportedVersion(self.version));
        }
        if self.model_id.trim().is_empty()
            || self.model_version.trim().is_empty()
            || self.decoder_sha256.trim().is_empty()
        {
            return Err(DecoderStateError::MissingIdentity);
        }
        if self.entries.is_empty() {
            return Err(DecoderStateError::EmptySchema);
        }

        let mut names = BTreeSet::new();
        let mut inputs = BTreeSet::new();
        let mut outputs = BTreeSet::new();
        for entry in &self.entries {
            for (value, seen) in [
                (&entry.name, &mut names),
                (&entry.onnx_input, &mut inputs),
                (&entry.onnx_output, &mut outputs),
            ] {
                if value.trim().is_empty() || !seen.insert(value.as_str()) {
                    return Err(DecoderStateError::DuplicateMapping(value.clone()));
                }
            }
            if entry.rank != entry.shape_semantics.len() {
                return Err(DecoderStateError::RankMismatch {
                    name: entry.name.clone(),
                    rank: entry.rank,
                    semantics: entry.shape_semantics.len(),
                });
            }
            if entry.step0_shape.len() != entry.rank || entry.step1_shape.len() != entry.rank {
                return Err(DecoderStateError::StepShapeRank(entry.name.clone()));
            }
            if let Some(axis) = entry.growth_axis {
                if axis >= entry.rank
                    || entry.shape_semantics[axis] != AxisSemantic::Sequence
                    || entry.encoder_static
                {
                    return Err(DecoderStateError::InvalidGrowthAxis(entry.name.clone()));
                }
            }
            let expected_attention = match entry.role {
                DecoderStateRole::SelfKey | DecoderStateRole::SelfValue => {
                    Some(AttentionKind::SelfAttention)
                }
                DecoderStateRole::CrossKey | DecoderStateRole::CrossValue => {
                    Some(AttentionKind::CrossAttention)
                }
                _ => None,
            };
            if let Some(expected) = expected_attention {
                if entry.attention_kind != Some(expected)
                    || entry.layer_index.is_none()
                    || (expected == AttentionKind::SelfAttention && entry.growth_axis.is_none())
                    || (expected == AttentionKind::CrossAttention
                        && (!entry.encoder_static || entry.growth_axis.is_some()))
                {
                    return Err(DecoderStateError::InvalidAttentionMetadata(
                        entry.name.clone(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn validate_transition(
        &self,
        before: &[DecoderStateObservation],
        after: &[DecoderStateObservation],
    ) -> Result<(), DecoderStateError> {
        self.validate()?;
        for entry in &self.entries {
            let before = observation(before, entry)?;
            let after = observation(after, entry)?;
            for observation in [before, after] {
                if observation.dtype != entry.dtype {
                    return Err(DecoderStateError::DTypeMismatch {
                        name: entry.name.clone(),
                        expected: entry.dtype,
                        actual: observation.dtype,
                    });
                }
                if observation.shape.len() != entry.rank {
                    return Err(DecoderStateError::ObservationRankMismatch {
                        name: entry.name.clone(),
                        expected: entry.rank,
                        actual: observation.shape.len(),
                    });
                }
            }
            for (axis, semantic) in entry.shape_semantics.iter().enumerate() {
                match semantic {
                    AxisSemantic::Batch | AxisSemantic::Beam
                        if before.shape[axis] != after.shape[axis] =>
                    {
                        return Err(DecoderStateError::BatchMismatch {
                            name: entry.name.clone(),
                            axis,
                            before: before.shape[axis],
                            after: after.shape[axis],
                        });
                    }
                    AxisSemantic::Sequence if entry.growth_axis == Some(axis) => {
                        if after.shape[axis] <= before.shape[axis] {
                            return Err(DecoderStateError::SequenceNotMonotonic {
                                name: entry.name.clone(),
                                axis,
                                before: before.shape[axis],
                                after: after.shape[axis],
                            });
                        }
                    }
                    AxisSemantic::StaticEncoderSequence
                        if before.shape[axis] != after.shape[axis] =>
                    {
                        return Err(DecoderStateError::SequenceNotMonotonic {
                            name: entry.name.clone(),
                            axis,
                            before: before.shape[axis],
                            after: after.shape[axis],
                        });
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

fn observation<'a>(
    observations: &'a [DecoderStateObservation],
    entry: &DecoderStateEntry,
) -> Result<&'a DecoderStateObservation, DecoderStateError> {
    observations
        .iter()
        .find(|observation| {
            observation.name == entry.name
                || observation.name == entry.onnx_input
                || observation.name == entry.onnx_output
        })
        .ok_or_else(|| DecoderStateError::UnknownObservation(entry.name.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn self_key_schema() -> DecoderStateSchema {
        DecoderStateSchema {
            version: 1,
            model_id: "fixture".to_owned(),
            model_version: "1".to_owned(),
            decoder_sha256: "a".repeat(64),
            entries: vec![DecoderStateEntry {
                name: "layer.0.self.key".to_owned(),
                paddle_variable: "cache_k_0".to_owned(),
                onnx_input: "past_key_0".to_owned(),
                onnx_output: "present_key_0".to_owned(),
                role: DecoderStateRole::SelfKey,
                dtype: DecoderDType::Float32,
                rank: 4,
                shape_semantics: vec![
                    AxisSemantic::Batch,
                    AxisSemantic::Head,
                    AxisSemantic::Sequence,
                    AxisSemantic::HeadDimension,
                ],
                step0_shape: vec![Some(1), Some(16), Some(0), Some(24)],
                step1_shape: vec![Some(1), Some(16), Some(1), Some(24)],
                growth_axis: Some(2),
                layer_index: Some(0),
                attention_kind: Some(AttentionKind::SelfAttention),
                update_rule: "append exactly one new token on sequence axis".to_owned(),
                encoder_static: false,
            }],
        }
    }

    #[test]
    fn valid_self_cache_requires_monotonic_sequence_growth() {
        let schema = self_key_schema();
        schema.validate().unwrap();
        let before = [DecoderStateObservation {
            name: "past_key_0".to_owned(),
            dtype: DecoderDType::Float32,
            shape: vec![1, 16, 1, 24],
        }];
        let after = [DecoderStateObservation {
            name: "present_key_0".to_owned(),
            dtype: DecoderDType::Float32,
            shape: vec![1, 16, 2, 24],
        }];
        schema.validate_transition(&before, &after).unwrap();
    }

    #[test]
    fn stagnant_self_cache_fails_closed() {
        let schema = self_key_schema();
        let observations = [DecoderStateObservation {
            name: "layer.0.self.key".to_owned(),
            dtype: DecoderDType::Float32,
            shape: vec![1, 16, 1, 24],
        }];
        assert!(matches!(
            schema.validate_transition(&observations, &observations),
            Err(DecoderStateError::SequenceNotMonotonic { .. })
        ));
    }

    #[test]
    fn cross_cache_cannot_declare_growth() {
        let mut schema = self_key_schema();
        schema.entries[0].role = DecoderStateRole::CrossKey;
        schema.entries[0].attention_kind = Some(AttentionKind::CrossAttention);
        schema.entries[0].encoder_static = true;
        assert!(matches!(
            schema.validate(),
            Err(DecoderStateError::InvalidGrowthAxis(_))
        ));
    }
}
