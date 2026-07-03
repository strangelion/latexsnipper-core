# Table Recognition Architecture

## Two-Layer Design

LaTeXSnipper uses a two-layer approach for table recognition:

```
Page Image
    ↓
[Layer 1: Table Detection] ─── TATR Detection (DETR)
    ↓
Table Region (cropped)
    ↓
[Layer 2: Structure Recognition] ─── SLANet_plus / TATR Structure / Projection
    ↓
Cell grid (row×col boundaries + rowspan/colspan)
    ↓
[OCR per cell] ─── text-rec (PP-OCRv6)
    ↓
TableBlock with TableCell content
```

## Model Recommendations

| Use Case | Layer 1 (Detection) | Layer 2 (Structure) |
|----------|---------------------|---------------------|
| Chinese Office docs | TATR Detection | **SLANet_plus** |
| Academic PDFs | TATR Detection | TATR Structure |
| MVP / lightweight | TATR Detection | Projection (no model) |

## Backend Selection

The structure recognition backend is configurable:

- **Rust code**: `TableStructureNode::with_backend("slanet")` or `with_backend("tatr")`
- **CLI test**: `STRUCT_BACKEND=slanet cargo test --test table_e2e -- --nocapture`
- **Default**: `tatr`

## SLANet_plus (Recommended for Chinese docs)

**Source**: PaddlePaddle/PaddleOCR PP-Structure v2

**Performance**:
- TEDS: 95.89% on PubTabNet
- Speed: ~766ms per image (CPU, MKL)

**Capabilities**:
- Wireless tables
- Complex merged cells
- Chinese + English mixed content
- Cell coordinate prediction

## TATR (Table Transformer)

**Source**: Microsoft Table Transformer

**Two checkpoints**:
- `microsoft/table-transformer-detection` (Layer 1)
- `microsoft/table-transformer-structure-recognition` (Layer 2)

**Training data**: PubTables-1M

**Output classes** (structure model):
| ID | Label |
|----|-------|
| 0 | no_object |
| 1 | table column |
| 2 | table row |
| 3 | table column header |
| 4 | table projected row header |
| 5 | table spanning cell |

**Pros**: Best for academic papers, complex merged cells

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Layer 1: Table Detection | Implemented | TATR DETR-based, 3 classes (no_obj/table/rotated) |
| Layer 2: Structure Recognition (TATR) | Implemented | DETR-based, 7 output classes, PubTables-1M |
| Layer 2: Structure Recognition (SLANet+) | Implemented | 50 HTML tokens + cell quads, PP-Structure v2 |
| Layer 2: Fallback (Projection) | Implemented | Line-based detection, no model needed |
| Cell OCR | Implemented | Uses existing text-det + text-rec (PP-OCRv6) |
| Grid building | Implemented | `build_grid_from_detections()` handles row/col/merge |
| Structured table output | Implemented | `TableStructureNode` + `TableRecognizerNode` pipeline |
| Export to HTML/Excel | Not started | |

## Rust Code Structure

```
crates/inference/src/
├── table_detector.rs      # Layer 1: YOLO-based detection (legacy)
├── table_structure.rs     # Layer 2: SLANet + unified interface
│   ├── preprocess_for_slanet()
│   ├── decode_slanet_output()
│   ├── recognize_structure_slanet()
│   └── recognize_table_structure()   # unified dispatch
├── tatr_recognizer.rs     # TATR detection + structure (shared inference)
│   ├── recognize_table_tatr()
│   └── build_grid_from_detections()
├── types.rs               # GridCell, DetectionBox, RecognitionResult
└── lib.rs                 # pub use exports

crates/pipeline/src/nodes/
├── detector_node.rs           # Layer 1 pipeline node
├── table_structure_node.rs    # Layer 2 node (backend-switchable)
└── table_recognizer_node.rs   # OCR + assembly
```

## Unified API

```rust
// Single entry point for structure recognition
pub fn recognize_table_structure(
    image: &SnipperImage,
    backend: &str,                      // "tatr" | "slanet" | "projection"
    backend_session: Option<&dyn InferenceSession>,
) -> Result<Option<Vec<GridCell>>>
```

Returns `GridCell` with row/col indices, rowspan/colspan, and bounding rect.
