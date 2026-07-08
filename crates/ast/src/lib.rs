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
    AnnotationBlock, Block, BorderStyle, CellAlignment, ChartBlock, CodeBlock, DescriptionItem,
    DescriptionListBlock, EmbeddedObjectBlock, FigureBlock, FloatBlock, FormulaBlock,
    HandwritingBlock, HeadingBlock, HorizontalRuleBlock, ListBlock, ListItem, MinipageBlock,
    ParagraphBlock, ProofBlock, QuoteBlock, ShapeBlock, TableBlock, TableCell, TextBoxBlock,
    TheoremBlock,
};
pub use builder::DocumentBuilder;
pub use document::{Document, Page};
pub use format::{
    CapabilityMatrix, ConversionContext, ExportArtifact, ExportOptions, FidelityLevel,
    FormatCapability, ImportOptions, LossKind, ModelCapability, ModelProviderKind,
    PdfExportMode, PdfExportOptions, RenderOptions,
};
pub use formula::{Formula, FormulaSource};
pub use formula_layout::{
    categorize_symbol, CommandInfo, EnvInfo, FormulaLayout, FormulaNode, SymbolCategory, SymbolInfo,
};
pub use geometry::{Point, Quad, Rect, Size};
pub use inline::{CodeInline, CiteStyle, ImageInline, Inline, LinkInline, SpanInline, TextRun};
pub use input::{
    InputFormat, InputSourceDescriptor, InputStorage, OfficeInsertKind, OutputLevel, PageRange,
    RecognizeInput, RecognizeOptions,
};
pub use media::{
    AssetBundle, AssetExportPolicy, AssetFormat, AssetId, AssetManifest, AssetManifestEntry,
    AssetResolver as MediaAssetResolver, AssetStorage, Diagnostic, DiagnosticLevel, ExportedAsset,
    MediaAsset, MediaRole,
};
pub use metadata::{Metadata, OcrMetadata};
pub use operation::Operation;
pub use report::{
    ArtifactEntry, ArtifactKind, ArtifactManifest, BlockSummary, ConfidenceSummary,
    CredentialRef, CredentialSource, DocumentReport, EventRecord, InputSummary, JobRoot,
    ProviderCallReport, ProviderReport, RetryPolicy, StageInput, StageKind, StageOutput,
    StageReport, StageSpec, StageStatus, UnsupportedFeature,
};
pub use span::{
    BlockPolicy, CoordinateSpace, NodeId, NodeIdGenerator, PdfSourceInfo, Position, Provenance,
    ProvenanceOperation, SourceInfo, Span,
};
pub use style::{
    AnnotationKind, BoxStyle, ChartAxis, ChartData, ChartLegend, ChartSeries, ChartType, Color,
    EmbeddedObjectKind, FontWeight, OfficeApp, OfficeSourceInfo, ParagraphStyle, ShapeStyle,
    ShapeType, TextAlignment, TextStyle, VerticalAlign,
};
pub use traits::{Exporter, Importer, OfficeAdapter, Renderer, SemanticConverter};
pub use visitor::{DocumentVisitor, TextCollector};
