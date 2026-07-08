use serde::{Deserialize, Serialize};

use crate::{Formula, SourceInfo};
use crate::style::TextStyle;

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
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strikethrough: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

impl TextRun {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
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
