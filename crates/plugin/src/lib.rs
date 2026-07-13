pub mod manifest;
pub mod plugin;
pub mod registry;
pub mod request;
pub mod response;

pub use manifest::{
    PluginClass, PluginDependency, PluginHook, PluginManifest, PluginPermissions,
    PLUGIN_API_VERSION,
};
pub use plugin::Plugin;
pub use registry::{PluginDiagnostic, PluginFailurePolicy, PluginRegistry, PluginRunResult};
pub use request::PluginRequest;
pub use response::PluginResponse;
