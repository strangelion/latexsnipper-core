pub mod execution;
pub mod manifest;
pub mod manifest_v3;
pub mod patch;
pub mod plugin;
pub mod process_host;
pub mod registry;
pub mod request;
pub mod response;
pub mod store;

pub use execution::{
    CancellationToken, DiagnosticSink, EffectivePermissionSummary, EffectivePluginPermissions,
    PluginExecutionClass, PluginExecutionContext, PluginExecutionNote,
};
pub use manifest::{
    PluginClass, PluginDependency, PluginHook, PluginManifest, PluginPermissions,
    PLUGIN_ABI_VERSION, PLUGIN_API_VERSION,
};
pub use manifest_v3::{
    NetworkDestinationV3, NetworkSchemeV3, PluginArtifactKindV3, PluginArtifactV3,
    PluginExecutionClassV3, PluginInterfaceVersionsV3, PluginManifestV3, PluginManifestV3Error,
    PluginPathAccessV3, PluginPathGrantV3, PluginPermissionsV3, PluginProvenanceV3,
    PluginRegistrationGrantsV3, PluginResourceLimitsV3, PluginSignatureV3,
    COMPONENT_WIT_VERSION_V1, PLUGIN_API_VERSION_FOR_MANIFEST_V3,
    PLUGIN_MANIFEST_SCHEMA_VERSION_V3, PROCESS_PLUGIN_PROTOCOL_VERSION_V1,
};
pub use patch::{ChangeSummary, DocumentPatch, DocumentView, PatchOperation};
pub use plugin::{PatchPlugin, Plugin, TransformPlugin};
pub use process_host::{
    IsolatedProcessHost, IsolatedProcessLimits, IsolatedProcessResult, IsolatedProcessStatus,
    ProcessPluginRequest, ProcessPluginResponse, PROCESS_PLUGIN_PROTOCOL_VERSION,
};
pub use registry::{
    PluginDiagnostic, PluginExecutionStatus, PluginFailurePolicy, PluginRegistry, PluginRunResult,
};
pub use request::PluginRequest;
pub use response::PluginResponse;
pub use store::{InstalledPlugin, PluginStore, PluginVerification};
