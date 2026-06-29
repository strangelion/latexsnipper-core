use serde::{Deserialize, Serialize};

use crate::Formula;

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

/// A run of text with optional styling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
}

/// An inline image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInline {
    pub image_data: Option<String>, // base64 or path
    pub width: Option<f32>,
    pub height: Option<f32>,
}
