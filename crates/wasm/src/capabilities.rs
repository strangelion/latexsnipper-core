use latexsnipper_conversion::{
    semantic_mime_type, CapabilityRegistry, CapabilityTarget, OutputFormat,
};
use latexsnipper_runtime::MemoryModelResolver;
use serde::Serialize;

use crate::api::ApiInfo;
use crate::profiles::{validate_profile, ProfileValidation};
use crate::state::{MemoryLimits, MemoryUsage};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapability {
    pub format: String,
    pub kind: &'static str,
    pub mime_type: &'static str,
    pub available: bool,
    pub binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDocument {
    pub api: ApiInfo,
    pub recognition: Vec<ProfileValidation>,
    pub exports: Vec<FormatCapability>,
    pub memory_limits: MemoryLimits,
    pub memory_usage: MemoryUsage,
    pub async_recognition: bool,
    pub progress_callbacks: bool,
    pub cancellation: CancellationCapability,
    pub worker_execution: WorkerExecutionCapability,
    pub indexed_db_cache: BrowserFeatureCapability,
    pub incremental_downloads: BrowserFeatureCapability,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancellationCapability {
    pub supported: bool,
    pub mode: &'static str,
    pub can_interrupt_active_inference: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerExecutionCapability {
    pub available_in_rust_package: bool,
    pub official_wrapper: &'static str,
    pub hard_cancellation_mode: &'static str,
    pub discards_model_sessions_on_termination: bool,
    pub max_concurrent_recognitions: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFeatureCapability {
    pub available_in_rust_package: bool,
    pub available_in_official_wrapper: bool,
    pub runtime_detection_required: bool,
}

pub fn collect(
    resolver: &MemoryModelResolver,
    limits: MemoryLimits,
    usage: MemoryUsage,
) -> CapabilityDocument {
    let recognition = [
        "formula",
        "text",
        "mixed",
        "formula_layout",
        "table",
        "handwriting",
    ]
    .into_iter()
    .filter_map(|profile| validate_profile(resolver, profile).ok())
    .collect();

    let exports = CapabilityRegistry::for_target(CapabilityTarget::Wasm32UnknownUnknown)
        .into_iter()
        .map(|entry| FormatCapability {
            format: entry.format.to_string(),
            kind: entry.kind,
            mime_type: entry.mime_type,
            available: entry.available,
            binary: entry.binary,
            reason: entry.unavailable_reason,
        })
        .collect();

    CapabilityDocument {
        api: ApiInfo::current(),
        recognition,
        exports,
        memory_limits: limits,
        memory_usage: usage,
        async_recognition: true,
        progress_callbacks: true,
        cancellation: CancellationCapability {
            supported: true,
            mode: "cooperative-stage-boundary",
            can_interrupt_active_inference: false,
        },
        worker_execution: WorkerExecutionCapability {
            available_in_rust_package: false,
            official_wrapper: "@latexsnipper/wasm-runtime",
            hard_cancellation_mode: "terminate-worker-and-restart",
            discards_model_sessions_on_termination: true,
            max_concurrent_recognitions: 1,
        },
        indexed_db_cache: BrowserFeatureCapability {
            available_in_rust_package: false,
            available_in_official_wrapper: true,
            runtime_detection_required: true,
        },
        incremental_downloads: BrowserFeatureCapability {
            available_in_rust_package: false,
            available_in_official_wrapper: true,
            runtime_detection_required: true,
        },
    }
}

pub fn collect_v3(
    resolver: &MemoryModelResolver,
    limits: MemoryLimits,
    usage: MemoryUsage,
) -> serde_json::Value {
    let mut value = serde_json::to_value(collect(resolver, limits, usage))
        .expect("capability document serialization must succeed");
    let object = value
        .as_object_mut()
        .expect("capability document must serialize as an object");
    object.remove("api");
    object.insert(
        "schemaVersion".to_string(),
        serde_json::Value::from(crate::api::CAPABILITY_VERSION_V3),
    );
    object.insert(
        "v2CompatibilityExports".to_string(),
        serde_json::Value::Bool(true),
    );
    value
}

pub fn mime_type(format: &str) -> &'static str {
    OutputFormat::all()
        .iter()
        .find(|candidate| candidate.name() == format)
        .map(|candidate| semantic_mime_type(*candidate))
        .unwrap_or("application/octet-stream")
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_runtime::MemoryModelResolver;

    #[test]
    fn wasm_metadata_matches_shared_registry_projection() {
        let document = collect(
            &MemoryModelResolver::new(),
            MemoryLimits::default(),
            MemoryUsage {
                artifact_count: 0,
                total_model_bytes: 0,
                pending_bytes: 0,
                session_bytes: None,
            },
        );
        let projected = CapabilityRegistry::for_target(CapabilityTarget::Wasm32UnknownUnknown);
        assert_eq!(document.exports.len(), projected.len());
        for entry in projected {
            let exported = document
                .exports
                .iter()
                .find(|candidate| candidate.format == entry.format)
                .unwrap();
            assert_eq!(exported.available, entry.available);
            assert_eq!(exported.binary, entry.binary);
            assert_eq!(exported.mime_type, entry.mime_type);
            assert_eq!(exported.reason, entry.unavailable_reason);
        }
    }

    #[test]
    fn v3_capability_projection_has_an_explicit_schema_and_no_v2_api_block() {
        let value = collect_v3(
            &MemoryModelResolver::new(),
            MemoryLimits::default(),
            MemoryUsage {
                artifact_count: 0,
                total_model_bytes: 0,
                pending_bytes: 0,
                session_bytes: None,
            },
        );
        assert_eq!(value["schemaVersion"], 3);
        assert!(value.get("api").is_none());
        assert_eq!(value["v2CompatibilityExports"], true);
    }
}
