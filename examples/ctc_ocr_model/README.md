# Custom CTC OCR Model Example

This example demonstrates how to package a custom CTC-based OCR model
for use with LaTeXSnipper Core's Model Package architecture.

## Directory Structure

```
my-ctc-model/
├── manifest.toml      # Model descriptor (required)
├── model.onnx          # Your trained ONNX model
└── charset.txt         # Character set (one char per line)
```

## manifest.toml Fields

| Field | Description |
|-------|-------------|
| `id` | Unique model identifier (category/variant format) |
| `task` | Model task: `TextRecognition`, `FormulaDetection`, etc. |
| `version` | Model version string |
| `adapter` | Adapter type: `ctc-recognition-v1`, `yolov8-detection-v1`, etc. |
| `input` | Input tensor specification |
| `output` | Output tensor specification |
| `files` | Model file paths (relative to manifest) |
| `preprocessing` | Image preprocessing config |
| `decoding` | Output decoding config |

## Supported Adapters

| Adapter | Description |
|---------|-------------|
| `ctc-recognition-v1` | CTC-based text recognition (CRNN, etc.) |
| `yolov8-detection-v1` | YOLOv8 object detection |
| `trocr-recognition-v1` | TrOCR encoder-decoder recognition |
| `dbnet-detection-v1` | DBNet text detection |
| `slanet-structure-v1` | SLANet table structure |

## Usage

1. Place your model files in a directory
2. Create a `manifest.toml` following the format above
3. Load the model registry:

```rust
use latexsnipper_runtime::ModelRegistry;

let registry = ModelRegistry::from_dir("my-models")?;

// Find a model by task
let models = registry.find_by_task(ModelTask::TextRecognition);

// Get a specific model
if let Some(manifest) = registry.get("custom/ctc-ocr") {
    println!("Found model: {} v{}", manifest.id, manifest.version);
}
```

## Character Set File Format

The `charset.txt` file should contain one character per line:

```
!
"
#
$
%
&
'
(
)
*
+
,
-
.
/
0
1
2
...
A
B
C
...
a
b
c
...
```

The blank token (CTC blank_id) is typically index 0.
