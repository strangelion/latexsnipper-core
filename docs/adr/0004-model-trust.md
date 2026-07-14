# ADR 0004: Model origin and trust

Status: accepted

No downloaded model becomes active before SHA-256 verification. Manifests pin the
official source revision, license, filename, checksum, size budgets, and mirrors.
Browser cache entries store their namespace version, checksum, profile, source URL,
byte length, and last-used time; every cache read is reverified.

The normal browser suite uses deterministic synthetic ONNX fixtures to prove the
full execution chain. A separately labelled scheduled test downloads the official
PaddlePaddle PP-LCNet document-orientation model and executes it under Tract/WASM.
That compatibility smoke does not make OCR accuracy claims.
