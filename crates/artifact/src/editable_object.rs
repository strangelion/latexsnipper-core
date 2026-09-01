use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const EDITABLE_OBJECT_SCHEMA: &str = "latexsnipper.object/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditableObjectKind {
    Formula,
    Drawing,
    CustomSymbol,
    Image,
    Table,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableObjectSource {
    pub format: String,
    pub value: Value,
    pub sha256: String,
}

impl EditableObjectSource {
    pub fn new(format: impl Into<String>, value: Value) -> Self {
        let sha256 = json_sha256(&value);
        Self {
            format: format.into(),
            value,
            sha256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableObjectPreview {
    pub format: String,
    pub mime_type: String,
    pub content_ref: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_points: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_points: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniversalEditableObject {
    pub schema: String,
    pub id: String,
    pub kind: EditableObjectKind,
    pub revision: u64,
    pub source: EditableObjectSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast: Option<Value>,
    #[serde(default)]
    pub previews: Vec<EditableObjectPreview>,
    pub core_version: String,
    pub checksum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_profile: Option<String>,
    pub created_with: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl UniversalEditableObject {
    pub fn new(
        id: impl Into<String>,
        kind: EditableObjectKind,
        source: EditableObjectSource,
        core_version: impl Into<String>,
    ) -> Self {
        let mut object = Self {
            schema: EDITABLE_OBJECT_SCHEMA.to_string(),
            id: id.into(),
            kind,
            revision: 0,
            source,
            ast: None,
            previews: Vec::new(),
            core_version: core_version.into(),
            checksum: String::new(),
            layout_profile: None,
            created_with: "LaTeXSnipper".to_string(),
            metadata: BTreeMap::new(),
        };
        object.refresh_checksum();
        object
    }

    pub fn refresh_checksum(&mut self) {
        self.checksum = self.expected_checksum();
    }

    pub fn refresh_integrity_metadata(&mut self) {
        self.source.sha256 = json_sha256(&self.source.value);
        self.refresh_checksum();
    }

    pub fn expected_checksum(&self) -> String {
        let mut value = serde_json::to_value(self).expect("editable object is serializable");
        if let Value::Object(object) = &mut value {
            object.insert("checksum".to_string(), Value::String(String::new()));
        }
        json_sha256(&value)
    }

    pub fn inspect(&self) -> EditableObjectHealthReport {
        let mut issues = Vec::new();
        let mut actions = Vec::new();

        if self.schema != EDITABLE_OBJECT_SCHEMA {
            issues.push(issue(
                EditableObjectIssueCode::UnsupportedSchema,
                EditableObjectIssueSeverity::Error,
                format!("unsupported editable object schema: {}", self.schema),
            ));
        }
        if self.id.trim().is_empty() {
            issues.push(issue(
                EditableObjectIssueCode::MissingId,
                EditableObjectIssueSeverity::Error,
                "editable object id is empty",
            ));
        }
        if self.source.format.trim().is_empty() || self.source.value.is_null() {
            issues.push(issue(
                EditableObjectIssueCode::MissingSource,
                EditableObjectIssueSeverity::Error,
                "editable object source is incomplete",
            ));
        } else if self.source.sha256 != json_sha256(&self.source.value) {
            issues.push(issue(
                EditableObjectIssueCode::SourceChecksumMismatch,
                EditableObjectIssueSeverity::Error,
                "editable object source checksum does not match its canonical value",
            ));
            actions.push(EditableObjectRepairAction::RecomputeIntegrityMetadata);
        }
        if self.core_version.trim().is_empty() {
            issues.push(issue(
                EditableObjectIssueCode::MissingCoreVersion,
                EditableObjectIssueSeverity::Error,
                "editable object core version is empty",
            ));
        }
        if self.previews.is_empty() {
            issues.push(issue(
                EditableObjectIssueCode::MissingPreview,
                EditableObjectIssueSeverity::Warning,
                "editable object has no visual preview",
            ));
            actions.push(EditableObjectRepairAction::RebuildPreview);
        }
        for (index, preview) in self.previews.iter().enumerate() {
            if preview.format.trim().is_empty()
                || preview.mime_type.trim().is_empty()
                || preview.content_ref.trim().is_empty()
                || !is_sha256(&preview.sha256)
                || preview
                    .width_points
                    .is_some_and(|value| value <= 0.0 || !value.is_finite())
                || preview
                    .height_points
                    .is_some_and(|value| value <= 0.0 || !value.is_finite())
            {
                issues.push(issue(
                    EditableObjectIssueCode::InvalidPreview,
                    EditableObjectIssueSeverity::Error,
                    format!("editable object preview {index} is incomplete or invalid"),
                ));
                actions.push(EditableObjectRepairAction::RebuildPreview);
            }
        }
        if self.checksum != self.expected_checksum() {
            issues.push(issue(
                EditableObjectIssueCode::ObjectChecksumMismatch,
                EditableObjectIssueSeverity::Error,
                "editable object checksum does not match its canonical envelope",
            ));
            actions.push(EditableObjectRepairAction::RecomputeIntegrityMetadata);
        }

        actions.sort();
        actions.dedup();
        let status = if issues.is_empty() {
            EditableObjectHealthStatus::Healthy
        } else if issues
            .iter()
            .any(|issue| issue.code == EditableObjectIssueCode::UnsupportedSchema)
        {
            EditableObjectHealthStatus::Unsupported
        } else if issues.iter().any(|issue| {
            matches!(
                issue.code,
                EditableObjectIssueCode::MissingId
                    | EditableObjectIssueCode::MissingSource
                    | EditableObjectIssueCode::MissingCoreVersion
            )
        }) {
            EditableObjectHealthStatus::Invalid
        } else {
            EditableObjectHealthStatus::Repairable
        };

        EditableObjectHealthReport {
            object_id: self.id.clone(),
            kind: self.kind,
            status,
            issues,
            repair_actions: actions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditableObjectHealthStatus {
    Healthy,
    Repairable,
    Invalid,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditableObjectIssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditableObjectIssueCode {
    UnsupportedSchema,
    MissingId,
    MissingSource,
    SourceChecksumMismatch,
    MissingCoreVersion,
    MissingPreview,
    InvalidPreview,
    ObjectChecksumMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableObjectIssue {
    pub code: EditableObjectIssueCode,
    pub severity: EditableObjectIssueSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditableObjectRepairAction {
    RecomputeIntegrityMetadata,
    RebuildPreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableObjectHealthReport {
    pub object_id: String,
    pub kind: EditableObjectKind,
    pub status: EditableObjectHealthStatus,
    pub issues: Vec<EditableObjectIssue>,
    pub repair_actions: Vec<EditableObjectRepairAction>,
}

fn issue(
    code: EditableObjectIssueCode,
    severity: EditableObjectIssueSeverity,
    message: impl Into<String>,
) -> EditableObjectIssue {
    EditableObjectIssue {
        code,
        severity,
        message: message.into(),
    }
}

fn json_sha256(value: &Value) -> String {
    let canonical = canonical_json(value);
    let bytes = serde_json::to_vec(&canonical).expect("canonical JSON is serializable");
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted = object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::to_value(sorted).expect("canonical map is serializable")
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(kind: EditableObjectKind) -> UniversalEditableObject {
        let mut object = UniversalEditableObject::new(
            "object-1",
            kind,
            EditableObjectSource::new("latex", json!({ "source": "x^2" })),
            "3.2.1",
        );
        object.previews.push(EditableObjectPreview {
            format: "svg".to_string(),
            mime_type: "image/svg+xml".to_string(),
            content_ref: "objects/object-1.svg".to_string(),
            sha256: "a".repeat(64),
            width_points: Some(72.0),
            height_points: Some(24.0),
        });
        object.refresh_checksum();
        object
    }

    #[test]
    fn every_supported_kind_round_trips_as_one_contract() {
        for kind in [
            EditableObjectKind::Formula,
            EditableObjectKind::Drawing,
            EditableObjectKind::CustomSymbol,
            EditableObjectKind::Image,
            EditableObjectKind::Table,
        ] {
            let object = object(kind);
            assert_eq!(object.inspect().status, EditableObjectHealthStatus::Healthy);
            let json = serde_json::to_string(&object).unwrap();
            let decoded: UniversalEditableObject = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.kind, kind);
            assert_eq!(decoded.checksum, object.checksum);
        }
    }

    #[test]
    fn source_mutation_is_detected_and_repairable() {
        let mut object = object(EditableObjectKind::Drawing);
        object.source.value = json!({ "source": "changed" });
        let report = object.inspect();
        assert_eq!(report.status, EditableObjectHealthStatus::Repairable);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == EditableObjectIssueCode::SourceChecksumMismatch));
        object.refresh_integrity_metadata();
        assert_eq!(object.inspect().status, EditableObjectHealthStatus::Healthy);
    }

    #[test]
    fn missing_preview_has_an_explicit_repair_plan() {
        let object = UniversalEditableObject::new(
            "formula-1",
            EditableObjectKind::Formula,
            EditableObjectSource::new("latex", json!("x")),
            "3.2.1",
        );
        let report = object.inspect();
        assert_eq!(report.status, EditableObjectHealthStatus::Repairable);
        assert_eq!(
            report.repair_actions,
            vec![EditableObjectRepairAction::RebuildPreview]
        );
    }

    #[test]
    fn envelope_checksum_is_independent_of_metadata_insertion_order() {
        let mut left = object(EditableObjectKind::Table);
        left.metadata.insert("z".to_string(), json!(1));
        left.metadata
            .insert("a".to_string(), json!({ "y": 2, "x": 1 }));
        left.refresh_checksum();

        let mut right = object(EditableObjectKind::Table);
        right
            .metadata
            .insert("a".to_string(), json!({ "x": 1, "y": 2 }));
        right.metadata.insert("z".to_string(), json!(1));
        right.refresh_checksum();
        assert_eq!(left.checksum, right.checksum);
    }

    #[test]
    fn unsupported_schema_is_not_silently_repaired() {
        let mut object = object(EditableObjectKind::CustomSymbol);
        object.schema = "latexsnipper.object/v99".to_string();
        assert_eq!(
            object.inspect().status,
            EditableObjectHealthStatus::Unsupported
        );
    }

    #[test]
    fn checked_in_contract_fixture_is_healthy() {
        let fixture = include_str!("../../../contracts/fixtures/editable-object-v1.json");
        let object: UniversalEditableObject = serde_json::from_str(fixture).unwrap();
        assert_eq!(object.schema, EDITABLE_OBJECT_SCHEMA);
        assert_eq!(object.inspect().status, EditableObjectHealthStatus::Healthy);
    }
}
