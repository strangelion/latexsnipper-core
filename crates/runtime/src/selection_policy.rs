use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AccelerationMode, ModelRegistry, ModelTask};

/// Runtime backend requested for a model selection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModelBackend {
    OnnxRuntime,
    Tract,
    RemoteApi,
}

/// Optimization goal used when more than one compatible model is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionPreference {
    Accuracy,
    Latency,
    #[default]
    Balanced,
}

/// Evidence readiness is deliberately distinct from simple package presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum ModelReadiness {
    #[default]
    Unknown,
    Available,
    Loadable,
    Tested,
    Validated,
}

/// Capabilities declared for a candidate model package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapability {
    pub backends: Vec<ModelBackend>,
    pub acceleration: Vec<AccelerationMode>,
    pub languages: Vec<String>,
}

/// Measured or reviewed evidence used for deterministic selection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelEvidence {
    pub readiness: ModelReadiness,
    pub accuracy_score: Option<f32>,
    pub warm_latency_ms: Option<u64>,
    pub memory_mb: Option<u64>,
}

/// Metadata kept outside the existing model manifest until its contract grows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelSelectionMetadata {
    pub capability: ModelCapability,
    pub evidence: ModelEvidence,
}

/// A normalized candidate that can be selected without loading a model package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCandidate {
    pub id: String,
    pub task: ModelTask,
}

/// Input to the model selection policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelectionRequest {
    pub task: ModelTask,
    pub backend: Option<ModelBackend>,
    pub acceleration: Option<AccelerationMode>,
    pub language: Option<String>,
    pub preference: SelectionPreference,
}

/// Explainable result of a selection decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelectionDecision {
    pub selected: Option<String>,
    pub fallbacks: Vec<String>,
    pub reasons: Vec<SelectionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionReason {
    RejectedTask {
        model_id: String,
    },
    RejectedBackend {
        model_id: String,
        backend: ModelBackend,
    },
    RejectedAcceleration {
        model_id: String,
        acceleration: AccelerationMode,
    },
    RejectedLanguage {
        model_id: String,
        language: String,
    },
    Selected {
        model_id: String,
        score: i64,
    },
    Fallback {
        model_id: String,
        score: i64,
    },
}

/// Deterministic selector layered on top of the existing registry.
#[derive(Debug, Clone, Default)]
pub struct ModelSelectionPolicy {
    metadata: BTreeMap<String, ModelSelectionMetadata>,
}

impl ModelSelectionPolicy {
    pub fn with_metadata(
        mut self,
        model_id: impl Into<String>,
        metadata: ModelSelectionMetadata,
    ) -> Self {
        self.metadata.insert(model_id.into(), metadata);
        self
    }

    pub fn select_registry(
        &self,
        registry: &ModelRegistry,
        request: &ModelSelectionRequest,
    ) -> ModelSelectionDecision {
        let candidates: Vec<ModelCandidate> = registry
            .find_by_task(request.task)
            .into_iter()
            .map(|(manifest, _)| ModelCandidate {
                id: manifest.id.clone(),
                task: manifest.task,
            })
            .collect();
        self.select(candidates, request)
    }

    pub fn select(
        &self,
        candidates: impl IntoIterator<Item = ModelCandidate>,
        request: &ModelSelectionRequest,
    ) -> ModelSelectionDecision {
        let mut reasons = Vec::new();
        let mut eligible = Vec::new();

        for candidate in candidates {
            if candidate.task != request.task {
                reasons.push(SelectionReason::RejectedTask {
                    model_id: candidate.id,
                });
                continue;
            }
            let metadata = self
                .metadata
                .get(&candidate.id)
                .cloned()
                .unwrap_or_default();
            if let Some(backend) = request.backend {
                if !metadata.capability.backends.is_empty()
                    && !metadata.capability.backends.contains(&backend)
                {
                    reasons.push(SelectionReason::RejectedBackend {
                        model_id: candidate.id,
                        backend,
                    });
                    continue;
                }
            }
            if let Some(acceleration) = request.acceleration {
                if !metadata.capability.acceleration.is_empty()
                    && !metadata.capability.acceleration.contains(&acceleration)
                {
                    reasons.push(SelectionReason::RejectedAcceleration {
                        model_id: candidate.id,
                        acceleration,
                    });
                    continue;
                }
            }
            if let Some(language) = request.language.as_deref() {
                if !metadata.capability.languages.is_empty()
                    && !metadata
                        .capability
                        .languages
                        .iter()
                        .any(|candidate_language| candidate_language.eq_ignore_ascii_case(language))
                {
                    reasons.push(SelectionReason::RejectedLanguage {
                        model_id: candidate.id,
                        language: language.to_string(),
                    });
                    continue;
                }
            }
            eligible.push((candidate.id, score(&metadata.evidence, request.preference)));
        }

        eligible.sort_by(|(left_id, left_score), (right_id, right_score)| {
            right_score
                .cmp(left_score)
                .then_with(|| left_id.cmp(right_id))
        });
        let selected = eligible.first().map(|(id, _)| id.clone());
        for (index, (model_id, score)) in eligible.into_iter().enumerate() {
            reasons.push(if index == 0 {
                SelectionReason::Selected { model_id, score }
            } else {
                SelectionReason::Fallback { model_id, score }
            });
        }
        let fallbacks = reasons
            .iter()
            .filter_map(|reason| match reason {
                SelectionReason::Fallback { model_id, .. } => Some(model_id.clone()),
                _ => None,
            })
            .collect();
        ModelSelectionDecision {
            selected,
            fallbacks,
            reasons,
        }
    }
}

fn score(evidence: &ModelEvidence, preference: SelectionPreference) -> i64 {
    let readiness = evidence.readiness as i64 * 1_000_000;
    let accuracy = evidence
        .accuracy_score
        .map(|value| (value * 10_000.0) as i64)
        .unwrap_or_default();
    let latency = evidence
        .warm_latency_ms
        .map(|value| 100_000_i64.saturating_sub(value as i64).max(0))
        .unwrap_or_default();
    match preference {
        SelectionPreference::Accuracy => readiness + accuracy * 10 + latency,
        SelectionPreference::Latency => readiness + latency * 10 + accuracy,
        SelectionPreference::Balanced => readiness + accuracy * 5 + latency * 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_deterministic_and_returns_fallbacks() {
        let policy = ModelSelectionPolicy::default()
            .with_metadata(
                "formula/cpu",
                ModelSelectionMetadata {
                    capability: ModelCapability {
                        backends: vec![ModelBackend::OnnxRuntime],
                        acceleration: vec![AccelerationMode::Cpu],
                        languages: vec!["en".to_string()],
                    },
                    evidence: ModelEvidence {
                        readiness: ModelReadiness::Validated,
                        accuracy_score: Some(0.95),
                        warm_latency_ms: Some(20),
                        memory_mb: None,
                    },
                },
            )
            .with_metadata(
                "formula/fast",
                ModelSelectionMetadata {
                    capability: ModelCapability {
                        backends: vec![ModelBackend::OnnxRuntime],
                        acceleration: vec![AccelerationMode::Cpu],
                        languages: vec!["en".to_string()],
                    },
                    evidence: ModelEvidence {
                        readiness: ModelReadiness::Tested,
                        accuracy_score: Some(0.90),
                        warm_latency_ms: Some(1),
                        memory_mb: None,
                    },
                },
            );
        let request = ModelSelectionRequest {
            task: ModelTask::FormulaRecognition,
            backend: Some(ModelBackend::OnnxRuntime),
            acceleration: Some(AccelerationMode::Cpu),
            language: Some("en".to_string()),
            preference: SelectionPreference::Accuracy,
        };

        let decision = policy.select(
            vec![
                ModelCandidate {
                    id: "formula/fast".to_string(),
                    task: ModelTask::FormulaRecognition,
                },
                ModelCandidate {
                    id: "formula/cpu".to_string(),
                    task: ModelTask::FormulaRecognition,
                },
            ],
            &request,
        );
        assert_eq!(decision.selected.as_deref(), Some("formula/cpu"));
        assert_eq!(decision.fallbacks, vec!["formula/fast"]);
    }

    #[test]
    fn incompatible_candidates_are_explained() {
        let request = ModelSelectionRequest {
            task: ModelTask::TextRecognition,
            backend: Some(ModelBackend::Tract),
            acceleration: None,
            language: None,
            preference: SelectionPreference::Balanced,
        };
        let decision = ModelSelectionPolicy::default()
            .with_metadata(
                "text/ort-only",
                ModelSelectionMetadata {
                    capability: ModelCapability {
                        backends: vec![ModelBackend::OnnxRuntime],
                        ..ModelCapability::default()
                    },
                    ..ModelSelectionMetadata::default()
                },
            )
            .select(
                vec![ModelCandidate {
                    id: "text/ort-only".to_string(),
                    task: ModelTask::TextRecognition,
                }],
                &request,
            );
        assert!(decision.selected.is_none());
        assert!(matches!(
            decision.reasons.as_slice(),
            [SelectionReason::RejectedBackend { .. }]
        ));
    }
}
