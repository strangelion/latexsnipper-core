pub mod block;
pub mod builder;
pub mod document;
pub mod format;
pub mod formula;
pub mod formula_layout;
pub mod geometry;
pub mod inline;
pub mod input;
pub mod media;
pub mod metadata;
pub mod operation;
pub mod report;
pub mod span;
pub mod style;
pub mod traits;
pub mod visitor;

pub use block::{
    AnnotationBlock, BibliographyBlock, BibliographyEntry, Block, BorderStyle, CellAlignment,
    CellDataType, ChartBlock, ChemicalFormulaBlock, CodeBlock, ColumnLayout, DataPoint,
    DescriptionItem, DescriptionListBlock, EmbeddedObjectBlock, FigureBlock, FloatBlock,
    FormFieldBlock, FormFieldKind, FormulaBlock, FormulaEnvironment, GraphBlock, GraphType,
    HandwritingBlock, HeaderFooterBlock, HeaderFooterKind, HeaderFooterScope, HeadingBlock,
    HorizontalRuleBlock, ListBlock, ListItem, MinipageBlock, PageBreakBlock, PageLayout,
    PageMargin, PageOrientation, ParagraphBlock, ProofBlock, QrCodeBlock, QuoteBlock, Revision,
    RevisionKind, SectionBreakBlock, SectionBreakKind, ShapeBlock, TableBlock, TableCell,
    TableCellStyle, TableColumn, TableRow, TableStyle, TextBoxBlock, TheoremBlock,
};
pub use builder::DocumentBuilder;
pub use document::{Document, NormalizeAssetOptions, Page, DOCUMENT_SCHEMA_VERSION};
pub use format::{
    CapabilityMatrix, ConversionContext, ExportArtifact, ExportFormat, ExportOptions,
    FidelityLevel, FormatCapability, GeneratedContent, ImportOptions, LossKind, ModelCapability,
    ModelProviderKind, PdfExportMode, PdfExportOptions, RenderOptions, SemanticFormat,
    TargetFormat,
};
pub use formula::{Formula, FormulaSource};
pub use formula_layout::{
    categorize_symbol, CommandInfo, EnvInfo, FormulaLayout, FormulaNode, SymbolCategory, SymbolInfo,
};
pub use geometry::{Point, Quad, Rect, Size};
pub use inline::{
    AnchorInline, CitationGroupInline, CitationItem, CiteStyle, CodeInline, CrossReferenceInline,
    CrossReferenceKind, DocumentOutline, ImageInline, Inline, LinkInline, LinkTarget,
    NoteDefinition, NoteKind, NoteRefInline, SpanInline, TextRun, TocEntry,
};
pub use input::{
    InputFormat, InputSourceDescriptor, InputStorage, OfficeInsertKind, OutputLevel, PageRange,
    RecognizeInput, RecognizeOptions,
};
pub use media::{
    AssetBundle, AssetExportPolicy, AssetExporter, AssetFormat, AssetId, AssetManifest,
    AssetManifestEntry, AssetReferenceResolver, AssetResolver as MediaAssetResolver, AssetStorage,
    AssetStore, AudioAsset, AudioFormat, Diagnostic, DiagnosticLevel, ExportedAsset, MediaAsset,
    MediaRole, VideoAsset, VideoFormat, E_API_CALL_FAILED, E_ENCRYPTED_FILE, E_INVALID_PACKAGE,
    E_MISSING_MODEL, E_RELATIONSHIP_ERROR, E_SCHEMA_VALIDATION_FAILED, I_LEGACY_IMAGE_MIGRATED,
    I_OCR_FALLBACK_USED, I_OPAQUE_OBJECT_PRESERVED, W_ACTIVEX_NOT_SUPPORTED,
    W_ASSET_DECODE_FAILURE, W_BLOCK_DOWNGRADED, W_CHART_DATA_SIMPLIFIED,
    W_EXTERNAL_DEPENDENCY_UNAVAILABLE, W_FORMULA_FALLBACK, W_FORM_FIELD_NOT_SUPPORTED,
    W_GPU_PROVIDER_FALLBACK, W_LAYOUT_LOSS, W_MEDIA_NOT_SUPPORTED, W_MISSING_ASSET_REF,
    W_MISSING_FONT, W_OLE_NOT_SUPPORTED, W_REVISION_NOT_FULLY_PRESERVED, W_SMARTART_NOT_SUPPORTED,
    W_STYLE_LOSS, W_UNSUPPORTED_FEATURE,
};
pub use metadata::{Metadata, OcrMetadata};
pub use operation::Operation;
pub use report::{
    ArtifactEntry, ArtifactKind, ArtifactManifest, BlockSummary, ConfidenceSummary,
    ConversionOutput, CredentialRef, CredentialSource, DocumentReport, EventRecord, InputSummary,
    JobRoot, ProviderCallReport, ProviderReport, RetryPolicy, StageInput, StageKind, StageOutput,
    StageProducedArtifact, StageReport, StageSpec, StageStatus, UnsupportedFeature,
};
pub use span::{
    BlockPolicy, CoordinateSpace, NodeId, NodeIdGenerator, PdfSourceInfo, Position, Provenance,
    ProvenanceOperation, SourceInfo, Span,
};
pub use style::{
    effective_text_style, AccessibilityInfo, AnnotationKind, BorderSide, BoxStyle, BulletStyle,
    ChartAxis, ChartData, ChartLegend, ChartSeries, ChartType, Color, EmbeddedObjectKind,
    FontWeight, LayerInfo, Length, LengthUnit, ListStyle, NumberingStyle, OfficeApp,
    OfficeSourceInfo, ParagraphStyle, PathCommand, ShapeGroup, ShapeStyle, ShapeType, TableBorder,
    TextAlignment, TextDirection, TextStyle, Transform2D, UnderlineStyle, VectorPath,
    VerticalAlign,
};
pub use traits::{Exporter, Importer, OfficeAdapter, Renderer, SemanticConverter, StageRunner};
pub use visitor::{DocumentVisitor, TextCollector};
