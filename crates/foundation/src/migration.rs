use serde::{Deserialize, Serialize};

/// Stable disposition for an explicit contract migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    Unchanged,
    Migrated,
    RequiresManualAction,
}

/// Structured warning emitted without silently changing source semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationWarning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl MigrationWarning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field: None,
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }
}

/// Version metadata and warnings for one bounded migration operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub source_contract: String,
    pub source_version: String,
    pub target_contract: String,
    pub target_version: String,
    pub status: MigrationStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<MigrationWarning>,
}

impl MigrationReport {
    pub fn new(
        source_contract: impl Into<String>,
        source_version: impl Into<String>,
        target_contract: impl Into<String>,
        target_version: impl Into<String>,
        status: MigrationStatus,
    ) -> Self {
        Self {
            source_contract: source_contract.into(),
            source_version: source_version.into(),
            target_contract: target_contract.into(),
            target_version: target_version.into(),
            status,
            warnings: Vec::new(),
        }
    }

    pub fn push_warning(&mut self, warning: MigrationWarning) {
        self.warnings.push(warning);
    }

    pub fn require_manual_action(&mut self, warning: MigrationWarning) {
        self.status = MigrationStatus::RequiresManualAction;
        self.push_warning(warning);
    }
}

/// Migrated value paired with the report required to interpret it safely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationOutcome<T> {
    pub value: T,
    pub report: MigrationReport,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_action_is_structured_and_serializable() {
        let mut report = MigrationReport::new(
            "plugin-manifest",
            "1",
            "plugin-manifest",
            "3",
            MigrationStatus::Migrated,
        );
        report.require_manual_action(
            MigrationWarning::new("MIGRATION_REVIEW_REQUIRED", "Review the network grant")
                .with_field("permissions.networkHosts"),
        );

        let encoded = serde_json::to_value(&report).unwrap();
        assert_eq!(encoded["status"], "requires_manual_action");
        assert_eq!(encoded["warnings"][0]["field"], "permissions.networkHosts");
    }
}
