use latexsnipper_ast::DOCUMENT_SCHEMA_VERSION;
use serde::Serialize;

/// Version of the stable JavaScript response contract.
pub const WASM_API_VERSION: u32 = 2;
/// Version of the capability document returned by this package.
pub const CAPABILITY_VERSION: u32 = 2;
/// Version of the Core 3 JavaScript response contract.
pub const WASM_API_VERSION_V3: u32 = latexsnipper_api_types::API_ENVELOPE_VERSION_V3;
/// Version of the Core 3 capability schema.
pub const CAPABILITY_VERSION_V3: u32 = latexsnipper_api_types::CAPABILITY_SCHEMA_VERSION_V3;
/// Current document schema produced by recognition.
pub const AST_SCHEMA_VERSION: &str = DOCUMENT_SCHEMA_VERSION;

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiInfoV3 {
    pub wasm_api_version: u32,
    pub capability_schema_version: u32,
    pub core_version: &'static str,
    pub document_schema_version: &'static str,
    pub v2_compatibility_exports: bool,
}

impl ApiInfoV3 {
    pub const fn current() -> Self {
        Self {
            wasm_api_version: WASM_API_VERSION_V3,
            capability_schema_version: CAPABILITY_VERSION_V3,
            core_version: env!("CARGO_PKG_VERSION"),
            document_schema_version: AST_SCHEMA_VERSION,
            v2_compatibility_exports: true,
        }
    }
}
