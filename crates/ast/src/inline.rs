use serde::{Deserialize, Serialize};

use crate::{Formula, SourceInfo};

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
}

impl Inline {
    /// Get source info for this inline element.
    pub fn source(&self) -> Option<&SourceInfo> {
        match self {
            Inline::Text(t) => t.source.as_ref(),
            Inline::Formula(f) => f.source_info.as_ref(),
            Inline::Image(i) => i.source.as_ref(),
        }
    }

    /// Set source info for this inline element.
    pub fn set_source(&mut self, source: SourceInfo) {
        match self {
            Inline::Text(t) => t.source = Some(source),
            Inline::Formula(f) => f.source_info = Some(source),
            Inline::Image(i) => i.source = Some(source),
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
    pub source: Option<SourceInfo>,
}

impl TextRun {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: None,
            italic: None,
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
}

/// An inline image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInline {
    pub image_data: Option<String>, // base64 or path
    pub width: Option<f32>,
    pub height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}
