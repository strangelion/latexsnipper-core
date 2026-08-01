//! Privacy-preserving intake and promotion rules for failure-driven corpora.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const FAILURE_CANDIDATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureCandidateStatus {
    New,
    Deduplicated,
    Minimized,
    Approved,
    Promoted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailureCandidate {
    pub schema_version: u32,
    pub candidate_id: String,
    pub source: String,
    pub input_hash: String,
    pub input_type: String,
    pub sanitized_input_ref: Option<String>,
    pub expected_ref: Option<String>,
    pub actual_ref: Option<String>,
    pub core_commit: String,
    pub model: Option<String>,
    pub runtime: Option<String>,
    pub provider: Option<String>,
    pub diagnostics: Vec<String>,
    pub failure_signature: String,
    pub ast_hash: Option<String>,
    pub redistributable: bool,
    pub license: Option<String>,
    pub sanitized: bool,
    pub reproducible: bool,
    pub has_expected_result: bool,
    pub error_classification: Option<String>,
    pub status: FailureCandidateStatus,
}

impl FailureCandidate {
    pub fn deduplication_key(&self) -> String {
        let canonical = format!(
            "{}\0{}\0{}\0{}\0{}\0{}",
            self.input_hash,
            self.ast_hash.as_deref().unwrap_or(""),
            self.failure_signature,
            self.provider.as_deref().unwrap_or(""),
            self.model.as_deref().unwrap_or(""),
            self.runtime.as_deref().unwrap_or("")
        );
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    }

    pub fn promotion_blockers(&self) -> Vec<&'static str> {
        let mut blockers = Vec::new();
        if !self.sanitized {
            blockers.push("input is not sanitized");
        }
        if !self.redistributable || self.license.as_deref().is_none_or(str::is_empty) {
            blockers.push("redistribution permission or license is missing");
        }
        if !self.reproducible {
            blockers.push("failure is not reproducible");
        }
        if !self.has_expected_result {
            blockers.push("expected result is missing");
        }
        if self
            .error_classification
            .as_deref()
            .is_none_or(str::is_empty)
        {
            blockers.push("error classification is missing");
        }
        blockers
    }

    pub fn can_promote(&self) -> bool {
        matches!(self.status, FailureCandidateStatus::Approved)
            && self.promotion_blockers().is_empty()
    }
}

pub fn first_structural_divergence(
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> Option<String> {
    first_divergence_at(left, right, "$")
}

fn first_divergence_at(
    left: &serde_json::Value,
    right: &serde_json::Value,
    path: &str,
) -> Option<String> {
    match (left, right) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let keys = left
                .keys()
                .chain(right.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for key in keys {
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => {
                        if let Some(found) =
                            first_divergence_at(left, right, &format!("{path}.{key}"))
                        {
                            return Some(found);
                        }
                    }
                    _ => return Some(format!("{path}.{key}")),
                }
            }
            None
        }
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                match (left.get(index), right.get(index)) {
                    (Some(left), Some(right)) => {
                        if let Some(found) =
                            first_divergence_at(left, right, &format!("{path}[{index}]"))
                        {
                            return Some(found);
                        }
                    }
                    _ => return Some(format!("{path}[{index}]")),
                }
            }
            None
        }
        _ => (left != right).then(|| path.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> FailureCandidate {
        FailureCandidate {
            schema_version: 1,
            candidate_id: "candidate-1".to_owned(),
            source: "parser-divergence".to_owned(),
            input_hash: "a".repeat(64),
            input_type: "latex".to_owned(),
            sanitized_input_ref: Some("minimized/input.tex".to_owned()),
            expected_ref: Some("minimized/expected.json".to_owned()),
            actual_ref: Some("minimized/actual.json".to_owned()),
            core_commit: "deadbeef".to_owned(),
            model: None,
            runtime: None,
            provider: None,
            diagnostics: Vec::new(),
            failure_signature: "ast:$[0].kind".to_owned(),
            ast_hash: Some("b".repeat(64)),
            redistributable: false,
            license: None,
            sanitized: true,
            reproducible: true,
            has_expected_result: true,
            error_classification: Some("parser-divergence".to_owned()),
            status: FailureCandidateStatus::Approved,
        }
    }

    #[test]
    fn private_candidate_never_promotes_automatically() {
        let candidate = candidate();
        assert!(!candidate.can_promote());
        assert!(candidate
            .promotion_blockers()
            .iter()
            .any(|reason| reason.contains("license")));
    }

    #[test]
    fn approved_redistributable_candidate_can_promote() {
        let mut candidate = candidate();
        candidate.redistributable = true;
        candidate.license = Some("CC0-1.0".to_owned());
        assert!(candidate.can_promote());
        assert_eq!(candidate.deduplication_key().len(), 64);
    }

    #[test]
    fn parser_divergence_reports_the_first_structural_path() {
        let left = serde_json::json!({"nodes": [{"kind": "fraction", "value": 1}]});
        let right = serde_json::json!({"nodes": [{"kind": "radical", "value": 1}]});
        assert_eq!(
            first_structural_divergence(&left, &right).as_deref(),
            Some("$.nodes[0].kind")
        );
    }
}
