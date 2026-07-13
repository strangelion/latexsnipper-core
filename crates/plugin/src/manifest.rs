use latexsnipper_ast::FormatCapability;
use serde::{Deserialize, Serialize};

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginHook {
    BeforeImport,
    AfterImport,
    BeforeRecognition,
    AfterRecognition,
    BeforeConversion,
    AfterConversion,
    BeforeExport,
    AfterExport,
    Validate,
    RegisterImporter,
    RegisterExporter,
    RegisterRuntime,
    RegisterModelAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginClass {
    BuiltInRust,
    NativeAbi,
    WasiComponent,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissions {
    #[serde(default)]
    pub filesystem_paths: Vec<String>,
    #[serde(default)]
    pub network_hosts: Vec<String>,
    #[serde(default)]
    pub environment_variables: Vec<String>,
    #[serde(default)]
    pub model_access: Vec<String>,
    pub memory_limit_bytes: Option<u64>,
    pub timeout_millis: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDependency {
    pub id: String,
    pub version_requirement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_api_version: u32,
    pub core_version_requirement: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Typed importer/exporter capabilities contributed while this plugin is enabled.
    #[serde(default)]
    pub format_capabilities: Vec<FormatCapability>,
    #[serde(default)]
    pub hooks: Vec<PluginHook>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    #[serde(default)]
    pub before: Vec<String>,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub architectures: Vec<String>,
    pub license: Option<String>,
    pub entrypoint: Option<String>,
    pub checksum_sha256: Option<String>,
    pub signature: Option<String>,
    pub configuration_schema: Option<serde_json::Value>,
    pub class: PluginClass,
}

impl PluginManifest {
    pub fn built_in(id: impl Into<String>, version: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            version: version.into(),
            plugin_api_version: PLUGIN_API_VERSION,
            core_version_requirement: format!("^{}", env!("CARGO_PKG_VERSION")),
            capabilities: Vec::new(),
            format_capabilities: Vec::new(),
            hooks: Vec::new(),
            priority: 0,
            dependencies: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            permissions: PluginPermissions::default(),
            platforms: Vec::new(),
            architectures: Vec::new(),
            license: None,
            entrypoint: None,
            checksum_sha256: None,
            signature: None,
            configuration_schema: None,
            class: PluginClass::BuiltInRust,
        }
    }
}
