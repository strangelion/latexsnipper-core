//! Custom math symbol domain.
//!
//! Defines the core `CustomMathSymbol` type, its metrics, variants,
//! assembly (stretchable symbols), transforms, assets and provenance.
//! Visual width is always derived from real glyph metrics — never from
//! character byte counts or UTF-8 length.

use serde::{Deserialize, Serialize};

/// Schema version of the custom symbol model.
pub const CUSTOM_SYMBOL_SCHEMA_VERSION: u32 = 1;

/// Reference to a binary asset (SVG, PNG, JPG).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolAssetRef {
    /// SHA-256 of the normalized asset bytes.
    pub sha256: String,
    /// MIME type of the asset (image/svg+xml, image/png, image/jpeg).
    pub mime_type: String,
    /// Byte length of the asset.
    pub byte_length: usize,
}

/// Mathematical symbol class (TeX-style spacing class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathSymbolClass {
    Ordinary,
    Operator,
    Binary,
    Relation,
    Opening,
    Closing,
    Punctuation,
    Inner,
    LargeOperator,
    Delimiter,
    Accent,
}

/// Bounding box of a glyph in font units (relative to the baseline origin).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphBoundingBox {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

/// A point in glyph coordinate space (x right, y up, origin at baseline).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Where limits (subscript/superscript over large operators) are placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitsMode {
    /// Limits above/below (display style for large operators).
    Limits,
    /// Limits as sub/superscripts to the right (text style).
    NoLimits,
    /// Ambiguous — caller decides.
    Inherit,
}

/// Typographic metrics of a custom glyph. All values are in font units
/// (units_per_em) unless noted. Visual width comes from `advance_width`
/// and `bounding_box` — never from byte counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathGlyphMetrics {
    pub units_per_em: u16,
    pub advance_width: f32,
    pub bounding_box: GlyphBoundingBox,
    /// Distance from baseline to the baseline of the glyph (usually 0).
    pub baseline: f32,
    /// Distance from baseline to the math axis.
    pub math_axis: f32,
    /// Italic correction (space added after the glyph in math mode).
    pub italic_correction: f32,
    /// Where an accent attaches on top (x relative to origin).
    pub top_accent_attachment: Option<f32>,
    /// Anchor for superscript placement.
    pub superscript_anchor: Option<Point>,
    /// Anchor for subscript placement.
    pub subscript_anchor: Option<Point>,
    /// Scale factor used in display style.
    pub display_scale: f32,
    /// Scale factor used in text style.
    pub text_scale: f32,
    /// Scale factor used in script style.
    pub script_scale: f32,
    /// Scale factor used in scriptscript style.
    pub scriptscript_scale: f32,
    pub limits_mode: LimitsMode,
}

impl Default for MathGlyphMetrics {
    fn default() -> Self {
        Self {
            units_per_em: 1000,
            advance_width: 500.0,
            bounding_box: GlyphBoundingBox {
                min_x: 0.0,
                min_y: -200.0,
                max_x: 500.0,
                max_y: 700.0,
            },
            baseline: 0.0,
            math_axis: 250.0,
            italic_correction: 0.0,
            top_accent_attachment: None,
            superscript_anchor: None,
            subscript_anchor: None,
            display_scale: 1.0,
            text_scale: 1.0,
            script_scale: 0.7,
            scriptscript_scale: 0.5,
            limits_mode: LimitsMode::NoLimits,
        }
    }
}

impl MathGlyphMetrics {
    /// Validate metrics: all scales positive and finite, advance > 0.
    pub fn validate(&self) -> Result<(), String> {
        if self.units_per_em == 0 {
            return Err("units_per_em must be non-zero".into());
        }
        for (name, value) in [
            ("advance_width", self.advance_width),
            ("display_scale", self.display_scale),
            ("text_scale", self.text_scale),
            ("script_scale", self.script_scale),
            ("scriptscript_scale", self.scriptscript_scale),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("{name} must be finite and positive, got {value}"));
            }
        }
        if self.bounding_box.max_x <= self.bounding_box.min_x
            || self.bounding_box.max_y <= self.bounding_box.min_y
        {
            return Err("bounding_box must have positive extent".into());
        }
        Ok(())
    }
}

/// One size variant of the glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathGlyphVariant {
    pub name: String,
    pub metrics: MathGlyphMetrics,
    pub asset: SymbolAssetRef,
}

/// A part of a stretchable (assembled) glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolPart {
    pub asset: SymbolAssetRef,
    /// Vertical offset of this part relative to the assembly origin.
    pub y_offset: f32,
}

/// Stretch assembly for extensible symbols (delimiters, arrows, accents).
/// The extender repeats between top/middle/bottom parts. When absent the
/// glyph may only be scaled with a fixed ratio and must be marked
/// `fixed_scale_only`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathGlyphAssembly {
    pub top: Option<SymbolPart>,
    pub middle: Option<SymbolPart>,
    pub bottom: Option<SymbolPart>,
    pub extender: SymbolPart,
    pub minimum_connector_overlap: f32,
}

/// Transformations applied to the canonical vector glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolTransforms {
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    /// Rotation in degrees (counter-clockwise).
    pub rotation_degrees: f32,
    /// Inherit the formula font's italic slant.
    pub inherit_italic_slant: bool,
    /// Custom slant angle in degrees (ignored when inherit_italic_slant).
    pub custom_slant_degrees: f32,
}

impl Default for SymbolTransforms {
    fn default() -> Self {
        Self {
            flip_horizontal: false,
            flip_vertical: false,
            rotation_degrees: 0.0,
            inherit_italic_slant: true,
            custom_slant_degrees: 0.0,
        }
    }
}

impl SymbolTransforms {
    /// Whether any non-identity transform is active.
    pub fn is_identity(&self) -> bool {
        !self.flip_horizontal
            && !self.flip_vertical
            && self.rotation_degrees == 0.0
            && self.custom_slant_degrees == 0.0
    }
}

/// Import parameters kept for provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportParameters {
    /// MIME type sniffed from content (not trusted from the filename).
    pub sniffed_mime_type: String,
    /// Whether the source was cropped to the ink bounds.
    pub cropped_to_ink: bool,
    /// Whether the background was removed (alpha mask generated).
    pub background_removed: bool,
    /// Original pixel dimensions before normalization.
    pub original_width: u32,
    pub original_height: u32,
}

/// Provenance of a custom symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolProvenance {
    /// SHA-256 of the original raw asset.
    pub source_asset_sha256: String,
    /// SHA-256 of the normalized canonical asset.
    pub canonical_asset_sha256: String,
    pub import_parameters: ImportParameters,
    /// Human-readable import source description.
    pub imported_from: Option<String>,
}

/// A custom math symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomMathSymbol {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub latex_command: Option<String>,
    pub unicode_scalar: Option<u32>,
    pub math_class: MathSymbolClass,
    pub source_asset: SymbolAssetRef,
    pub canonical_svg: SymbolAssetRef,
    pub preview_png: Option<SymbolAssetRef>,
    pub metrics: MathGlyphMetrics,
    pub variants: Vec<MathGlyphVariant>,
    pub assembly: Option<MathGlyphAssembly>,
    pub transforms: SymbolTransforms,
    pub provenance: SymbolProvenance,
}

impl CustomMathSymbol {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        math_class: MathSymbolClass,
        source_asset: SymbolAssetRef,
        canonical_svg: SymbolAssetRef,
        metrics: MathGlyphMetrics,
        provenance: SymbolProvenance,
    ) -> Self {
        Self {
            schema_version: CUSTOM_SYMBOL_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            aliases: Vec::new(),
            latex_command: None,
            unicode_scalar: None,
            math_class,
            source_asset,
            canonical_svg,
            preview_png: None,
            metrics,
            variants: Vec::new(),
            assembly: None,
            transforms: SymbolTransforms::default(),
            provenance,
        }
    }

    /// Validate the symbol for storage/transfer.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("symbol id must not be empty".into());
        }
        if self.name.trim().is_empty() {
            return Err("symbol name must not be empty".into());
        }
        self.metrics.validate()?;
        for variant in &self.variants {
            variant.metrics.validate()?;
        }
        if let Some(assembly) = &self.assembly {
            if assembly.minimum_connector_overlap < 0.0 {
                return Err("assembly minimum_connector_overlap must be non-negative".into());
            }
        }
        Ok(())
    }
}

/// Whether a symbol only supports fixed-ratio scaling (no assembly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleCapability {
    /// May stretch using the assembly (top/middle/bottom/extender).
    Stretchable,
    /// May only be scaled at a fixed ratio (no assembly).
    FixedScaleOnly,
}

impl CustomMathSymbol {
    /// Determine scaling capability from the assembly presence.
    pub fn scale_capability(&self) -> ScaleCapability {
        if self.assembly.is_some() {
            ScaleCapability::Stretchable
        } else {
            ScaleCapability::FixedScaleOnly
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> SymbolAssetRef {
        SymbolAssetRef {
            sha256: "abc123".into(),
            mime_type: "image/svg+xml".into(),
            byte_length: 42,
        }
    }

    fn provenance() -> SymbolProvenance {
        SymbolProvenance {
            source_asset_sha256: "src".into(),
            canonical_asset_sha256: "canon".into(),
            import_parameters: ImportParameters {
                sniffed_mime_type: "image/svg+xml".into(),
                cropped_to_ink: true,
                background_removed: false,
                original_width: 100,
                original_height: 100,
            },
            imported_from: None,
        }
    }

    #[test]
    fn valid_symbol_roundtrips() {
        let symbol = CustomMathSymbol::new(
            "rel-approx",
            "approx",
            MathSymbolClass::Relation,
            asset(),
            asset(),
            MathGlyphMetrics::default(),
            provenance(),
        );
        assert!(symbol.validate().is_ok());
        let json = serde_json::to_string(&symbol).unwrap();
        let back: CustomMathSymbol = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "rel-approx");
        assert_eq!(back.math_class, MathSymbolClass::Relation);
    }

    #[test]
    fn invalid_metrics_rejected() {
        let metrics = MathGlyphMetrics {
            advance_width: 0.0,
            ..MathGlyphMetrics::default()
        };
        let symbol = CustomMathSymbol::new(
            "bad",
            "bad",
            MathSymbolClass::Ordinary,
            asset(),
            asset(),
            metrics,
            provenance(),
        );
        assert!(symbol.validate().is_err());
    }

    #[test]
    fn assembly_presence_drives_scale_capability() {
        let base = CustomMathSymbol::new(
            "s",
            "s",
            MathSymbolClass::Delimiter,
            asset(),
            asset(),
            MathGlyphMetrics::default(),
            provenance(),
        );
        assert_eq!(base.scale_capability(), ScaleCapability::FixedScaleOnly);

        let stretchy = CustomMathSymbol {
            assembly: Some(MathGlyphAssembly {
                top: None,
                middle: None,
                bottom: None,
                extender: SymbolPart {
                    asset: asset(),
                    y_offset: 0.0,
                },
                minimum_connector_overlap: 2.0,
            }),
            ..base
        };
        assert_eq!(stretchy.scale_capability(), ScaleCapability::Stretchable);
    }

    #[test]
    fn transforms_identity_detection() {
        assert!(SymbolTransforms::default().is_identity());
        let t = SymbolTransforms {
            flip_horizontal: true,
            ..SymbolTransforms::default()
        };
        assert!(!t.is_identity());
    }
}
