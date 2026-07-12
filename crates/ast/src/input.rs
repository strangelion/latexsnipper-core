use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// InputFormat
// ---------------------------------------------------------------------------

/// Supported input formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputFormat {
    RawPixels,
    ImagePng,
    ImageJpeg,
    ImageWebp,
    ImageBmp,
    ImageTiff,
    ImageGif,
    ImageSvg,
    Pdf,
    OfficeDocx,
    OfficePptx,
    OfficeXlsx,
    Html,
    Markdown,
    Latex,
    Typst,
    MathML,
    OMML,
    JsonAst,
    PlainText,
    Clipboard,
    Unknown,
}

// ---------------------------------------------------------------------------
// InputStorage
// ---------------------------------------------------------------------------

/// Storage location for input data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum InputStorage {
    FilePath(String),
    Bytes(Vec<u8>),
    Uri(String),
    Clipboard,
    /// Input from an Office application selection.
    OfficeSelection(crate::OfficeSourceInfo),
}

// ---------------------------------------------------------------------------
// InputSourceDescriptor
// ---------------------------------------------------------------------------

/// Descriptor for an input source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSourceDescriptor {
    /// The format of the input.
    pub format: InputFormat,
    /// Where the input data is stored.
    pub storage: InputStorage,
    /// Original filename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Page range to process (for multi-page documents).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_range: Option<String>,
    /// DPI for rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
    /// Optional credential reference for password-protected inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// RecognizeInput / RecognizeOptions / OutputLevel
// ---------------------------------------------------------------------------

/// The input to a recognition pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecognizeInput {
    Image(String), // placeholder: image path / bytes ref
    Source(InputSourceDescriptor),
    Document(crate::Document),
}

/// Configuration options for recognition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecognizeOptions {
    pub page_range: Option<crate::PageRange>,
    pub preserve_assets: bool,
    pub preserve_layout: bool,
    pub enable_api_enhance: bool,
    pub provider: Option<String>,
    pub prompt_profile: Option<String>,
    pub output_level: OutputLevel,
}

impl Default for RecognizeOptions {
    fn default() -> Self {
        Self {
            page_range: None,
            preserve_assets: true,
            preserve_layout: false,
            enable_api_enhance: false,
            provider: None,
            prompt_profile: None,
            output_level: OutputLevel::BlocksWithLayout,
        }
    }
}

/// How much detail to include in the recognition output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputLevel {
    TextOnly,
    Blocks,
    BlocksWithLayout,
    FullDocument,
    FullDocumentWithAssets,
}

// ---------------------------------------------------------------------------
// OfficeInsertKind
// ---------------------------------------------------------------------------

/// Office insertion kind for clipboard/insert operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfficeInsertKind {
    OMath,
    OoxmlFragment,
    HtmlClipboard,
    RtfClipboard,
    ImagePng,
    ImageSvg,
    NativeShape,
    NativeTable,
}

// ---------------------------------------------------------------------------
// PageRange
// ---------------------------------------------------------------------------

/// A page range specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRange {
    /// Start page (1-based).
    pub start: u32,
    /// End page (1-based, inclusive).
    pub end: u32,
}
