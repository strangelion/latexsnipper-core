use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AssetId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub String);

// ---------------------------------------------------------------------------
// AssetFormat — raw file/byte formats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetFormat {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Gif,
    Svg,
    Pdf,
    Emf,
    Wmf,
    Heic,
    RawPixels,
    /// An OOXML part (e.g. a relation target inside .docx/.pptx).
    OoxmlPart,
    /// A PDF image XObject reference.
    PdfXObject,
    Unknown,
}

// ---------------------------------------------------------------------------
// MediaRole — semantic role of an image/asset within the document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaRole {
    Photo,
    Screenshot,
    Scan,
    Diagram,
    Flowchart,
    Chart,
    Plot,
    Icon,
    Logo,
    Signature,
    Stamp,
    FormulaRender,
    EmbeddedObjectPreview,
    Decorative,
    Background,
    Watermark,
    Unknown,
}

// ---------------------------------------------------------------------------
// AssetStorage — where the asset's byte content lives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum AssetStorage {
    InlineBase64 {
        data: String,
    },
    FilePath {
        path: String,
    },
    Uri {
        uri: String,
    },
    BytesRef {
        id: String,
    },
    OfficeRelationship {
        r_id: String,
        part_name: Option<String>,
        content_type: Option<String>,
    },
    PdfObject {
        object_id: Option<String>,
        page_index: Option<usize>,
        xobject_name: Option<String>,
    },
    Clipboard {
        format: String,
    },
}

// ---------------------------------------------------------------------------
// MediaAsset
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: AssetId,
    pub format: AssetFormat,
    pub mime_type: Option<String>,
    pub role: MediaRole,
    pub storage: AssetStorage,

    pub width: Option<f32>,
    pub height: Option<f32>,
    pub dpi: Option<f32>,
    pub color_space: Option<String>,
    pub checksum_sha256: Option<String>,
    pub alt_text: Option<String>,

    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Diagnostic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Diagnostic code constants (Office/PDF/API conversion warnings)
// ---------------------------------------------------------------------------

/// SmartArt graphics are not fully convertible; preview only.
pub const W_SMARTART_NOT_SUPPORTED: &str = "W_SMARTART_NOT_SUPPORTED";
/// OLE embedded objects are not convertible; placeholder used.
pub const W_OLE_NOT_SUPPORTED: &str = "W_OLE_NOT_SUPPORTED";
/// Office chart data may be simplified in conversion.
pub const W_CHART_DATA_SIMPLIFIED: &str = "W_CHART_DATA_SIMPLIFIED";
/// Audio/video embedded content is not supported.
pub const W_MEDIA_NOT_SUPPORTED: &str = "W_MEDIA_NOT_SUPPORTED";
/// ActiveX controls are not supported.
pub const W_ACTIVEX_NOT_SUPPORTED: &str = "W_ACTIVEX_NOT_SUPPORTED";
/// Form fields are not fully convertible.
pub const W_FORM_FIELD_NOT_SUPPORTED: &str = "W_FORM_FIELD_NOT_SUPPORTED";
/// Tracked changes / revisions are not fully preserved.
pub const W_REVISION_NOT_FULLY_PRESERVED: &str = "W_REVISION_NOT_FULLY_PRESERVED";
/// A block type was silently dropped during conversion.
pub const W_BLOCK_DOWNGRADED: &str = "W_BLOCK_DOWNGRADED";
/// Legacy image_data was automatically migrated to MediaAsset.
pub const I_LEGACY_IMAGE_MIGRATED: &str = "I_LEGACY_IMAGE_MIGRATED";
/// Reference to an asset ID that does not exist in Document.assets.
pub const W_MISSING_ASSET_REF: &str = "W_MISSING_ASSET_REF";
/// An API call failed.
pub const E_API_CALL_FAILED: &str = "E_API_CALL_FAILED";
/// JSON schema validation failed.
pub const E_SCHEMA_VALIDATION_FAILED: &str = "E_SCHEMA_VALIDATION_FAILED";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
    pub source: Option<crate::SourceInfo>,
    #[serde(default)]
    pub recoverable: bool,
    #[serde(default)]
    pub data: serde_json::Value,
}

impl Diagnostic {
    pub fn new(
        level: DiagnosticLevel,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            code: code.into(),
            message: message.into(),
            source: None,
            recoverable: false,
            data: serde_json::Value::Null,
        }
    }

    pub fn with_source(mut self, source: crate::SourceInfo) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }
}

// ---------------------------------------------------------------------------
// ExportedAsset — describes a resolved/copied asset after export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedAsset {
    pub asset_id: AssetId,
    pub relative_path: String,
    pub format: AssetFormat,
    pub mime_type: Option<String>,
    pub checksum_sha256: Option<String>,
}

// ---------------------------------------------------------------------------
// AssetExportPolicy — how an asset should be treated during export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetExportPolicy {
    /// Keep original bytes/reference unchanged.
    PreserveOriginal,
    /// Copy to a designated assets directory.
    CopyToAssetsDir,
    /// Embed as base64 in the output.
    EmbedBase64,
    /// Rasterize SVG to PNG/JPEG.
    RasterizeSvg,
    /// Rasterize any vector format (EMF, WMF, SVG).
    RasterizeVector,
    /// Drop decorative assets silently.
    DropDecorative,
    /// Replace with a placeholder.
    UsePlaceholder,
}

// ---------------------------------------------------------------------------
// Three-level asset resolution traits
// ---------------------------------------------------------------------------

/// Level 1: Where assets live — raw storage access.
pub trait AssetStore {
    /// Retrieve raw bytes for a given asset.
    fn get_bytes(&self, id: &AssetId) -> std::result::Result<Vec<u8>, String>;
    /// Look up the full asset metadata.
    fn get_asset(&self, id: &AssetId) -> Option<&MediaAsset>;
}

/// Level 2: How to reference an asset in the current output format.
/// Each output format (HTML, Markdown, LaTeX, etc.) resolves asset IDs
/// to format-specific reference strings (data URIs, file paths, etc.).
pub trait AssetReferenceResolver {
    /// Resolve an asset ID to a string reference for the current output format.
    fn resolve_reference(&self, id: &AssetId) -> std::result::Result<String, String>;
}

/// Level 3: How to export/copy an asset to a target directory.
pub trait AssetExporter {
    /// Export an asset to a target directory, returning metadata about the export.
    fn export_asset(
        &self,
        id: &AssetId,
        target_dir: &std::path::Path,
    ) -> std::result::Result<ExportedAsset, String>;
}

// ---------------------------------------------------------------------------
// Backward-compatible composite trait
// ---------------------------------------------------------------------------

/// Composite trait covering all three asset resolution levels.
/// New code should implement `AssetStore`, `AssetReferenceResolver`, and `AssetExporter` individually.
/// This trait is automatically implemented for any type that implements all three.
pub trait AssetResolver: AssetStore + AssetReferenceResolver + AssetExporter {}

impl<T> AssetResolver for T where T: AssetStore + AssetReferenceResolver + AssetExporter {}

// ---------------------------------------------------------------------------
// AssetBundle — a collection of assets for bulk export / clipboard bundles
// ---------------------------------------------------------------------------

/// A bundle of assets collected for bulk export.
///
/// TODO(phase1): implement `Document::normalize_assets()` that walks all blocks,
///   collects asset references, fills checksums/mime/role/size from actual content,
///   and populates the Document's asset manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBundle {
    pub assets: Vec<MediaAsset>,
    pub bundle_format: String,
    pub checksum: Option<String>,
}

// ---------------------------------------------------------------------------
// AssetManifest — index of all assets referenced by a document
// ---------------------------------------------------------------------------

/// Index of every asset referenced by a document, with dedup info.
/// Intended to live alongside the Document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifest {
    pub schema_version: String,
    pub entries: Vec<AssetManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifestEntry {
    pub asset_id: AssetId,
    pub checksum_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    /// IDs of other entries that share the same content (dedup).
    #[serde(default)]
    pub dedup_group: Vec<AssetId>,
}
