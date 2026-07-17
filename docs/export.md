# Export Crate

> Export capability -- RenderTree + SVG/PNG/PDF generators + portable visual rendering

## Core principles

1. **Document -> RenderTree -> Generator -> GeneratedContent -> ExportArtifact**
2. **RenderTree avoids repeated AST traversal**
3. **Generators are pluggable; new formats only require implementing a trait**
4. **RenderBundle provides portable visual rendering with policy-enforced SVG validation**

## Architecture

```
Document
   |
   v
RenderTree
   |
   v
Generator
   |
   v
GeneratedContent
   |
   v
ExportArtifact

SVG input / Document
   |
   v
RenderPreference
   |
   v
RenderBundle
   +-- preferred ExportArtifact
   +-- fallback ExportArtifact(s)
```

## Modules

| Module | File | Description |
|---|---|---|
| `render_tree` | render_tree.rs | RenderTree intermediate representation |
| `generator` | generator.rs | Generator trait |
| `svg` | svg.rs | SVG Generator |
| `png` | png.rs | SVG -> PNG deterministic renderer |
| `pdf` | pdf.rs | PDF Generator (lopdf) |
| `text` | text.rs | Plain Text Generator |
| `bundle` | bundle.rs | RenderPreference / RenderBundle / RenderDimensions |
| `svg_policy` | svg_policy.rs | SVG validation, normalization, vector policy |
| `service` | service.rs | ExportService unified entry point |

## Key types

### Generator trait

```rust
pub trait Generator {
    fn generate(&self, tree: &RenderTree) -> Result<GeneratedContent>;
    fn extension(&self) -> &str;
    fn mime_type(&self) -> &str;
    fn name(&self) -> &str;
}
```

### RenderPreference

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPreference {
    #[default]
    Auto,       // SVG preferred, PNG fallback
    VectorOnly, // SVG only, reject embedded raster
    RasterOnly, // PNG only
}
```

### RenderBundle

```rust
pub struct RenderBundle {
    pub preferred: ExportArtifact,
    pub fallbacks: Vec<ExportArtifact>,
    pub dimensions: RenderDimensions,
}
```

### SvgContentPolicy

```rust
pub enum SvgContentPolicy {
    AllowEmbeddedRaster,
    VectorOnly,
}
```

### ExportService

```rust
pub struct ExportService;

impl ExportService {
    // Single-format export
    pub fn export(doc: &Document, format: VisualFormat) -> Result<ExportArtifact>;
    pub fn to_svg(doc: &Document) -> Result<ExportArtifact>;
    pub fn to_pdf(doc: &Document) -> Result<ExportArtifact>;
    pub fn to_png(doc: &Document) -> Result<ExportArtifact>;
    pub fn to_text(doc: &Document) -> Result<ExportArtifact>;

    // Portable bundle from Document
    pub fn render_bundle(doc: &Document, preference: RenderPreference) -> Result<RenderBundle>;

    // Portable bundle from pre-existing SVG (e.g. MathJax)
    pub fn render_bundle_from_svg(svg: &str, preference: RenderPreference) -> Result<RenderBundle>;
}
```

## SVG policy

The `svg_policy` module provides a unified SVG parsing and validation layer:

- `validate_svg()` -- validates SVG input against a content policy
- `normalize_svg()` -- parses and rewrites SVG into canonical usvg representation
- External image file references are always rejected (no filesystem access)
- `VectorOnly` policy rejects embedded raster images (JPEG, PNG, GIF, WebP)
- Data URL images remain available for self-contained SVG

## PDF Generator

Uses `lopdf 0.44` to generate and re-validate PDF files. Built-in Helvetica,
Helvetica Bold, and Courier Type 1 fonts are used for text rendering; characters
outside WinAnsi range currently degrade to `?`, so full Unicode font embedding
remains a pre-GA fidelity task.

### Supported document elements

| Element | Rendering |
|---------|-----------|
| Heading | HelveticaBold, size decreasing by level |
| Paragraph | Helvetica 11pt |
| Formula | LaTeX source text display ($...$ / $$...$$) |
| Table | Equal-width columns, 9pt font |
| List | Bullet / numbered |
| Code | Courier monospace |
| Quote | Vertical line prefix |
| HR | Horizontal line |

## Page selection

### AST layer methods

```rust
impl Document {
    pub fn filter_pages(&self, indices: &[usize]) -> Document;
    pub fn filter_page_numbers(&self, numbers: &[u32]) -> Document;
    pub fn parse_page_range(range: &str) -> Vec<u32>;
}
```

## Tests

- PDF generator: valid PDF header, metadata, all node types
- PNG generator: valid PNG header, reopenable binary
- SVG policy: malformed SVG rejection, external image rejection, vector-only validation, normalization roundtrip
- Service bundle: Auto/VectorOnly/RasterOnly bundle composition
- RenderBundle: dimensions pt conversion, default preference

## Dependencies

```
Export
  depends on AST, Syntax, lopdf, resvg, sha2, serde
  depended on by Engine (indirectly)
```
