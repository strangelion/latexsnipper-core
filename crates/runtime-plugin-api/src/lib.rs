//! Stable C ABI and trusted in-process loader for custom inference runtimes.
//!
//! Loading native code is never driven by a model package. Applications first
//! install a plugin into a separate directory, explicitly enroll its canonical
//! path and SHA-256 in [`RuntimePluginTrustStore`], then run discovery.

pub mod abi;
pub mod descriptor;
pub mod discovery;
pub mod error;
mod host;
pub mod trust;

pub use descriptor::RuntimePluginDescriptor;
pub use discovery::{
    DiscoveryIssue, DiscoveryIssueKind, RuntimePluginDiscovery, RuntimePluginDiscoveryReport,
    RUNTIME_PLUGIN_DESCRIPTOR,
};
pub use host::RuntimePluginFactory;
pub use trust::{RuntimePluginTrustStore, TrustedRuntimePlugin};
