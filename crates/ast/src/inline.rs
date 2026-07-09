use serde::{Deserialize, Serialize};

use crate::style::TextStyle;
use crate::{Formula, SourceInfo};

/// Citation style for academic references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CiteStyle {
    /// \cite{key}
    Plain,
    /// \citet{key} (author in text)
    Author,
    /// \citep{key} (parenthetical)
    Parenthetical,
}

/// An inline element within a paragraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Inline {
    /// A run of text.
    Text(TextRun),
    /// An inline formula.
    Formula(Formula),
    /// An inline image.
    Image(ImageInline),
    /// A footnote: \footnote{content}.
    Footnote {
        /// Footnote content.
        content: Box<Inline>,
    },
    /// A reference to a note (footnote/endnote).
    NoteRef(NoteRefInline),
    /// A label: \label{key} (not rendered, used for cross-references).
    Label {
        /// Reference key.
        key: String,
    },
    /// A reference: \ref{key} or \eqref{key}.
    Reference {
        /// Reference key.
        key: String,
        /// True if equation reference (\eqref).
        eq_ref: bool,
    },
    /// A citation: \cite{key}.
    Citation {
        /// Citation key.
        key: String,
        /// Citation style.
        style: CiteStyle,
    },
    /// A hard line break.
    LineBreak,
    /// A soft line break (paragraph break within same block).
    SoftBreak,
    /// A styled span containing nested inlines.
    Span(SpanInline),
    /// A hyperlink.
    Link(LinkInline),
    /// An inline code span.
    Code(CodeInline),
    /// An anchor/target for hyperlinks.
    Anchor(AnchorInline),
    /// A cross-reference to a labeled element.
    CrossReference(CrossReferenceInline),
    /// A group of citations.
    CitationGroup(CitationGroupInline),
    /// Superscript content.
    Superscript(Vec<Inline>),
    /// Subscript content.
    Subscript(Vec<Inline>),
}

impl Inline {
    /// Get source info for this inline element.
    pub fn source(&self) -> Option<&SourceInfo> {
        match self {
            Inline::Text(t) => t.source.as_ref(),
            Inline::Formula(f) => f.source_info.as_ref(),
            Inline::Image(i) => i.source.as_ref(),
            _ => None,
        }
    }

    /// Set source info for this inline element.
    pub fn set_source(&mut self, source: SourceInfo) {
        match self {
            Inline::Text(t) => t.source = Some(source),
            Inline::Formula(f) => f.source_info = Some(source),
            Inline::Image(i) => i.source = Some(source),
            _ => {}
        }
    }
}

/// A run of text with optional styling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<TextStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

impl TextRun {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
            bold: None,
            italic: None,
            underline: None,
            strikethrough: None,
            source: None,
        }
    }

    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = Some(bold);
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = Some(italic);
        self
    }

    pub fn with_underline(mut self, underline: bool) -> Self {
        self.underline = Some(underline);
        self
    }

    pub fn with_strikethrough(mut self, strikethrough: bool) -> Self {
        self.strikethrough = Some(strikethrough);
        self
    }
}

/// An inline image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInline {
    /// Reference to a media asset in the document's asset collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<crate::AssetId>,
    /// DEPRECATED: Use `asset_id` instead. This field is kept for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_data: Option<String>, // base64 or path
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// Alternative text description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A styled span containing nested inlines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanInline {
    /// Nested inline content.
    pub content: Vec<Inline>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<TextStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// A hyperlink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInline {
    /// Link text content.
    pub content: Vec<Inline>,
    /// Target URL or path.
    pub target: String,
    /// Optional tooltip/title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<LinkTarget>,
}

impl LinkInline {
    pub fn effective_target(&self) -> LinkTarget {
        self.link_target.clone().unwrap_or_else(|| {
            if self.target.starts_with("#") {
                LinkTarget::InternalAnchor(self.target[1..].to_string())
            } else if self.target.starts_with("mailto:") {
                LinkTarget::Email(self.target[7..].to_string())
            } else {
                LinkTarget::Url(self.target.clone())
            }
        })
    }

    pub fn target_string(&self) -> String {
        self.link_target
            .as_ref()
            .map(|lt| match lt {
                LinkTarget::Url(u) => u.clone(),
                LinkTarget::InternalAnchor(a) => format!("#{}", a),
                LinkTarget::Email(e) => format!("mailto:{}", e),
                LinkTarget::File(f) => f.clone(),
                LinkTarget::Custom(c) => c.clone(),
            })
            .unwrap_or_else(|| self.target.clone())
    }
}

/// An inline code span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeInline {
    /// The code content.
    pub code: String,
    /// Optional language hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// An anchor/target for hyperlinks (e.g., HTML `<a name="...">`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorInline {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

/// Kind of cross-reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrossReferenceKind {
    Figure,
    Table,
    Equation,
    Section,
    Page,
    Bookmark,
    Custom,
}

/// A cross-reference to a labeled element (e.g., "see Figure 3").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossReferenceInline {
    pub target_id: String,
    pub kind: CrossReferenceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,
}

/// A single citation key with optional prefix/suffix/locator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationItem {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
}

/// A group of citations (e.g., `\cite{key1,key2}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationGroupInline {
    pub citations: Vec<CitationItem>,
    pub style: CiteStyle,
}

use crate::Block;

/// Target of a hyperlink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LinkTarget {
    Url(String),
    InternalAnchor(String),
    Email(String),
    File(String),
    Custom(String),
}

/// Kind of note (footnote or endnote).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteKind {
    Footnote,
    Endnote,
}

/// A reference to a note (footnote/endnote) in the document body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRefInline {
    pub note_id: String,
    pub kind: NoteKind,
    pub source: Option<SourceInfo>,
}

/// Content of a note (footnote/endnote), stored in the Document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteDefinition {
    pub id: String,
    pub kind: NoteKind,
    pub content: Vec<Block>,
    pub source: Option<SourceInfo>,
}

// ---------------------------------------------------------------------------
// TocEntry / DocumentOutline
// ---------------------------------------------------------------------------

/// A single entry in a table of contents or document outline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    pub title: String,
    pub level: u8,
    pub page_number: Option<u32>,
    pub anchor_id: Option<String>,
    pub children: Vec<TocEntry>,
}

/// The full document outline (table of contents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentOutline {
    pub title: Option<String>,
    pub entries: Vec<TocEntry>,
}
