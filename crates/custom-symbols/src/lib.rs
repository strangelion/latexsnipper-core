//! Custom math symbol domain for LaTeXSnipper.
//!
//! - [`symbol`]: `CustomMathSymbol`, `MathGlyphMetrics`, variants,
//!   stretch assembly, transforms, assets, provenance
//! - [`pack`]: secure `*.lsymbolpack` ZIP validation
//! - [`transfer`]: neutral formula / custom-symbol transfer bundles

pub mod composition;
pub mod pack;
pub mod symbol;
pub mod transfer;
pub mod transforms;

pub use composition::{
    CompositionLayer, CompositionLayerSource, CompositionTransform, DrawingPrimitive,
    MathGlyphComposition, COMPOSITION_SCHEMA_VERSION, MAX_COMPOSITION_LAYERS,
    MAX_FORMULA_SOURCE_BYTES,
};
pub use pack::{
    validate_symbol_pack_archive, SymbolPackManifest, SymbolPackValidationError,
    SYMBOL_PACK_MANIFEST_NAME,
};
pub use symbol::{
    CustomMathSymbol, GlyphBoundingBox, ImportParameters, LimitsMode, MathGlyphAssembly,
    MathGlyphMetrics, MathGlyphVariant, MathSymbolClass, Point, ScaleCapability, SymbolAssetRef,
    SymbolPart, SymbolProvenance, SymbolTransforms, CUSTOM_SYMBOL_SCHEMA_VERSION,
};
pub use transfer::{
    BinaryArtifact, CustomSymbolTransferBundle, FormulaTransferBundle,
    TRANSFER_BUNDLE_SCHEMA_VERSION,
};
pub use transforms::{apply_transforms, TransformedGlyph};
