# Office and PDF fidelity framework

## Contract

Every callable format pair exposes six independent measurements:

- `structuralValidity`
- `semanticPreservation`
- `layoutPreservation`
- `visualFidelity`
- `editability`
- `roundTripFidelity`

Each measurement has a claim, optional score, evidence identifiers, and explicit
limitations. `not-measured` and `unsupported` are first-class results. No aggregate
boolean is allowed to replace the six measurements. This backward-compatible
addition advances the executable capability schema from `2.0.0` to `2.1.0`;
missing dimension data still deserializes to `not-measured`.

## Golden corpora

`fidelity/corpora/index.json` pins the SHA-256, source, license, required feature
evidence, diagnostics, assets, and opaque parts for repository-generated DOCX,
PPTX, XLSX, and PDF fixtures. `generate-fidelity-fixtures` deterministically
recreates them. Corpus validation rejects duplicate IDs, missing format coverage,
missing required feature evidence, checksum drift, absolute paths, drive-prefixed
paths, backslashes, and parent traversal.

The fixtures intentionally combine supported and unsupported constructs. The goal
is to measure what survives and which diagnostics are emitted, not to imply full
Microsoft Office or PDF-viewer fidelity.

## Evidence layers

1. Reopen validation checks only package/PDF structure.
2. Semantic AST comparison measures page, block, node-type, and text snapshots.
3. Expected diagnostic comparison requires stable warnings for unsupported content.
4. Asset preservation checks imported and reopened non-opaque assets.
5. Opaque-part preservation compares required OOXML part bytes exactly.
6. Optional rendering compares RGBA output and records failure artifacts.
7. Optional application smoke delegates to an installed Office/viewer harness.

Layers 6 and 7 are skipped, rather than passed, when their external harnesses are
not available. CI always uploads the JSON evidence report and uploads visual
artifacts on failure.

## Capability documentation

`docs/generated/fidelity-capabilities.md` is generated from
`DocumentExportService::capability_matrix()`. CI regenerates the file and rejects
drift, so the documented Office/PDF pair claims cannot silently diverge from the
callable registry.
