pub mod execution;
pub mod manifest;
pub mod manifest_v3;
pub mod patch;
pub mod plugin;
pub mod process_host;
pub mod registry;
#[cfg(feature = "registry-network")]
pub mod registry_manager;
pub mod remote_plugin_store;
pub mod request;
pub mod response;
pub mod signed_registry;
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
#[cfg(feature = "registry-network")]
pub use registry_manager::{
    ConfiguredRegistry, RegistryRefreshResult, RegistrySearchResult, SignedRegistryManager,
};
pub use remote_plugin_store::{
    extract_and_verify_remote_package, verify_staged_remote_package, RemoteInstalledPlugin,
    RemotePackageLimits, RemotePluginProvenance, RemotePluginStore, RemoteStoreDoctor,
    VerifiedRemotePackage,
};
pub use request::PluginRequest;
pub use response::PluginResponse;
pub use signed_registry::{
    canonical_signed_bytes, canonical_signed_envelope_bytes, decode_signed_metadata,
    metadata_description, verify_description, verify_initial_root, verify_metadata,
    verify_registry_chain, verify_root_update, MetadataDescription, MetadataKey, MetadataRole,
    MetadataSignature, RegistryError, RegistryRole, RegistryTarget, RegistryVersions, RootMetadata,
    SignedMetadata, SnapshotMetadata, TargetMetadata, TargetsMetadata, TimestampMetadata,
    TrustState, VerifiedRegistryChain,
};
#[cfg(feature = "registry-network")]
pub use signed_registry::{HttpsRegistryDownloader, RegistryDownloadKind};
pub use store::{InstalledPlugin, PluginStore, PluginVerification};
