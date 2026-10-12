//! Default-deny host for versioned LaTeXSnipper WebAssembly components.

mod activation;
pub mod approval;
pub mod bindings;
pub mod diagnostic;
pub mod formula_conversion;
pub mod formula_profile;
pub mod host;
pub mod limits;
pub mod package;
pub mod permissions;
#[cfg(windows)]
mod write_compression;
mod write_security;
#[cfg(windows)]
mod write_streams;

pub use activation::ActivatedRemoteWasiPlugin;
pub use approval::{
    ApprovalGrantSnapshot, ApprovalRequest, ApprovalScope, SensitiveAction,
    SensitiveActionController,
};
pub use formula_conversion::{WasiFormulaBackend, WASI_DOCUMENT_JSON_MEDIA_TYPE};
pub use formula_profile::{PreparedFormulaRegistry, WasiFormulaProfile};

pub use diagnostic::{
    WasiDiagnostic, WasiDiagnosticCode, WasiDiagnosticDetail, WasiDiagnosticSeverity,
};
pub use host::{
    CompiledWasiComponent, ComponentInvocation, ComponentInvocationResult, DenyNetworkBroker,
    NetworkBroker, NetworkRequest, NetworkResponse, WasiComponentHost,
};
pub use limits::{WasiHostPolicy, WasiResourceLimits, WasiResourceMinimums};
pub use package::{
    verify_component_artifact_bytes, VerifiedComponentPackage, WasiComponentPackageVerifier,
    WasiPackagePolicy,
};
pub use permissions::{
    ComponentNetworkScheme, ComponentPermissions, FilesystemGrant, FilesystemOperationError,
    NetworkGrant,
};

/// Stable WIT package version consumed by this host.
pub const COMPONENT_WIT_PACKAGE_VERSION: &str = "1.0.0";

pub use bindings::latexsnipper::plugin::types as wit_types;
