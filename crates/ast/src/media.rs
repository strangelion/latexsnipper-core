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
// AssetResolver trait — resolves asset IDs to bytes/paths/data URIs
// ---------------------------------------------------------------------------

/// Resolver that converts an AssetId into concrete byte content or file paths.
pub trait AssetResolver {
    /// Resolve the asset's raw bytes.
    fn resolve_bytes(&self, id: &AssetId) -> std::result::Result<Vec<u8>, String>;

    /// Resolve to a local file path, if available.
    fn resolve_path(&self, id: &AssetId)
        -> std::result::Result<Option<std::path::PathBuf>, String>;

    /// Build a data URI string (e.g. "data:image/png;base64,...").
    fn resolve_data_uri(&self, id: &AssetId) -> std::result::Result<String, String>;

    /// Export the asset to a target directory, returning metadata.
    fn export_asset(
        &self,
        id: &AssetId,
        target_dir: &std::path::Path,
    ) -> std::result::Result<ExportedAsset, String>;
}

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
