//! Default-deny host for versioned LaTeXSnipper WebAssembly components.

pub mod bindings;
pub mod diagnostic;
pub mod host;
pub mod limits;
pub mod package;
pub mod permissions;

pub use diagnostic::{WasiDiagnostic, WasiDiagnosticCode};
pub use host::{
    CompiledWasiComponent, ComponentInvocation, ComponentInvocationResult, DenyNetworkBroker,
    NetworkBroker, NetworkRequest, NetworkResponse, WasiComponentHost,
};
pub use limits::WasiResourceLimits;
pub use package::{
    verify_component_artifact_bytes, VerifiedComponentPackage, WasiComponentPackageVerifier,
};
pub use permissions::{
    ComponentNetworkScheme, ComponentPermissions, FilesystemGrant, NetworkGrant,
};

/// Stable WIT package version consumed by this host.
pub const COMPONENT_WIT_PACKAGE_VERSION: &str = "1.0.0";

pub use bindings::latexsnipper::plugin::types as wit_types;
