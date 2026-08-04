//! Neutral transfer bundles.
//!
//! Core only produces neutral bundles — it never writes to a system
//! clipboard. Hosts (Office, WPS, plugins) consume these bundles and are
//! responsible for real clipboard / insertion behavior.

use serde::{Deserialize, Serialize};

use crate::symbol::CustomMathSymbol;

/// Schema version of transfer bundles.
pub const TRANSFER_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// A binary artifact carried inside a bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryArtifact {
    pub mime_type: String,
    /// Base64-encoded content.
    pub data_base64: String,
    /// SHA-256 of the raw bytes.
    pub sha256: String,
}

/// Neutral formula transfer bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaTransferBundle {
    pub schema_version: u32,
    pub latex: String,
    pub markdown: String,
    pub mathml: Option<String>,
    pub omml: Option<String>,
    pub html: Option<String>,
    pub svg: Option<BinaryArtifact>,
    pub png: Option<BinaryArtifact>,
    /// LaTeXSnipper native protocol JSON.
    pub protocol_json: String,
}

impl FormulaTransferBundle {
    pub fn new(
        latex: impl Into<String>,
        markdown: impl Into<String>,
        protocol_json: String,
    ) -> Self {
        Self {
            schema_version: TRANSFER_BUNDLE_SCHEMA_VERSION,
            latex: latex.into(),
            markdown: markdown.into(),
            mathml: None,
            omml: None,
            html: None,
            svg: None,
            png: None,
            protocol_json,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != TRANSFER_BUNDLE_SCHEMA_VERSION {
            return Err(format!(
                "formula bundle schema version {} != {}",
                self.schema_version, TRANSFER_BUNDLE_SCHEMA_VERSION
            ));
        }
        if self.latex.trim().is_empty() {
            return Err("formula bundle latex must not be empty".into());
        }
        Ok(())
    }
}

/// Neutral custom-symbol transfer bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSymbolTransferBundle {
    pub schema_version: u32,
    pub symbol: CustomMathSymbol,
    pub pack_ref: Option<String>,
    pub svg: BinaryArtifact,
    pub png: Option<BinaryArtifact>,
    /// LaTeX fallback (alias command or symbol name).
    pub latex_fallback: String,
    /// LaTeXSnipper native protocol JSON.
    pub protocol_json: String,
}

impl CustomSymbolTransferBundle {
    pub fn new(
        symbol: CustomMathSymbol,
        svg: BinaryArtifact,
        protocol_json: String,
        latex_fallback: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: TRANSFER_BUNDLE_SCHEMA_VERSION,
            symbol,
            pack_ref: None,
            svg,
            png: None,
            latex_fallback: latex_fallback.into(),
            protocol_json,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != TRANSFER_BUNDLE_SCHEMA_VERSION {
            return Err(format!(
                "symbol bundle schema version {} != {}",
                self.schema_version, TRANSFER_BUNDLE_SCHEMA_VERSION
            ));
        }
        self.symbol.validate()?;
        if self.svg.data_base64.is_empty() {
            return Err("symbol bundle svg must not be empty".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::*;

    fn asset() -> SymbolAssetRef {
        SymbolAssetRef {
            sha256: "abc".into(),
            mime_type: "image/svg+xml".into(),
            byte_length: 3,
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
                original_width: 10,
                original_height: 10,
            },
            imported_from: None,
        }
    }

    fn symbol() -> CustomMathSymbol {
        CustomMathSymbol::new(
            "sym",
            "sym",
            MathSymbolClass::Ordinary,
            asset(),
            asset(),
            MathGlyphMetrics::default(),
            provenance(),
        )
    }

    #[test]
    fn formula_bundle_roundtrip() {
        let bundle = FormulaTransferBundle::new(r"x^2", r"$x^2$", r"{}".into());
        assert!(bundle.validate().is_ok());
        let json = serde_json::to_string(&bundle).unwrap();
        let back: FormulaTransferBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.latex, r"x^2");
        assert_eq!(back.schema_version, 1);
    }

    #[test]
    fn symbol_bundle_roundtrip() {
        let bundle = CustomSymbolTransferBundle::new(
            symbol(),
            BinaryArtifact {
                mime_type: "image/svg+xml".into(),
                data_base64: "PHN2Zz4=".into(),
                sha256: "sha".into(),
            },
            r"{}".into(),
            "\\mySym",
        );
        assert!(bundle.validate().is_ok());
        let json = serde_json::to_string(&bundle).unwrap();
        let back: CustomSymbolTransferBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.latex_fallback, "\\mySym");
        assert_eq!(back.symbol.math_class, MathSymbolClass::Ordinary);
    }

    #[test]
    fn empty_svg_rejected() {
        let bundle = CustomSymbolTransferBundle::new(
            symbol(),
            BinaryArtifact {
                mime_type: "image/svg+xml".into(),
                data_base64: String::new(),
                sha256: "sha".into(),
            },
            r"{}".into(),
            "x",
        );
        assert!(bundle.validate().is_err());
    }
}
