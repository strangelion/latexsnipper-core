pub mod execution;
pub mod manifest;
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
