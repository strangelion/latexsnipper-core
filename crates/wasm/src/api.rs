use serde::Serialize;

/// Version of the stable JavaScript response contract.
pub const WASM_API_VERSION: u32 = 2;
/// Version of the capability document returned by this package.
pub const CAPABILITY_VERSION: u32 = 2;
/// Current document schema produced by recognition.
pub const AST_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiInfo {
    pub wasm_api_version: u32,
    pub capability_version: u32,
    pub core_version: &'static str,
    pub schema_version: &'static str,
}

impl ApiInfo {
    pub const fn current() -> Self {
        Self {
            wasm_api_version: WASM_API_VERSION,
            capability_version: CAPABILITY_VERSION,
            core_version: env!("CARGO_PKG_VERSION"),
            schema_version: AST_SCHEMA_VERSION,
        }
    }
}
