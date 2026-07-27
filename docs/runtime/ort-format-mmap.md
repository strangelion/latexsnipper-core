# ORT-format and model mmap capability

## Implemented boundary

The ONNX Runtime factory advertises these positive capabilities:

```text
artifact-format:onnx
artifact-format:ort
model-loading:file
```

A model variant can therefore require `artifact-format:ort` and fail closed on
a runtime that does not advertise it. ONNX remains the default. A variant may
declare format alternatives with role suffixes:

```json
{
  "artifacts": {
    "decoder.onnx": "decoder.onnx",
    "decoder.ort": "decoder.ort"
  },
  "options": {
    "preferOrtFormat": true
  },
  "capabilities": ["artifact-format:ort"]
}
```

Adapters still request the logical role `decoder`; the runtime owns format
selection. Only `.onnx` and `.ort` are accepted by this factory.

## mmap status

Model mmap is not enabled by default. The
`memory-map-model-experimental` feature now provides `ModelMemoryOwner`,
`RuntimeSessionEntry`, and `RuntimeSessionOwnerCache`. The cache entry declares
the session before its optional mapping owner, clears both together, and lets
active callers retain an old version while a new version is atomically
published. Lifecycle tests cover clear and old/new coexistence.

An isolated `runtime-mmap-experimental` compatibility feature (an alias for the
new feature) maps a versioned model and uses ORT's borrowed
`commit_from_memory_directly` API. It runs first/warm inference, atomically
switches a version pointer, proves the old session remains usable, loads the new
version, records process memory and Windows delete/replace behavior, and checks
temporary-directory cleanup. Accordingly:

- `model-loading:mmap` is not advertised;
- `memoryMapModel: true` fails with `ONNX_MODEL_MMAP_UNSUPPORTED`;
- no RSS or cold-start improvement is attributed to mmap.

The owner abstraction and experiment are positive lifecycle evidence, but not
proof of an RSS or cold-start improvement. The ONNX factory continues to reject
`memoryMapModel: true`: rc.12 exposes a borrowed in-memory session, and wiring
that self-reference into the production factory without a sound owner is not
accepted. Keeping the advertised capability disabled prevents silent opt-in.

## Update and locking design

Production mmap enablement must keep the mapping in a session-owned,
self-referential lifetime container, close it only after the last task releases
the old session, and install models through a validated staging directory. The
installer must verify archive limits,
package-relative paths, schema version, and SHA-256 before an atomic rename.
The validated update design uses immutable, hash-versioned files plus an atomic
pointer switch. It does not depend on overwriting a mapped artifact.

The existing model importer already enforces checksums, traversal/symlink
rejection, archive count/size/ratio budgets, staging cleanup, and executable
artifact exclusion. Production cache integration and live task handoff remain
future work.

## I/O mechanics benchmark

`tools/runtime/benchmark_model_loading.py` reports buffered read, mmap open,
page-touch timings, working-set snapshots, checksum, environment, and file
replacement behavior:

```powershell
python tools/runtime/benchmark_model_loading.py `
  models/formula-rec/trocr-deit/encoder_model.onnx `
  --iterations 5 `
  --output benchmarks/runtime-loading/v1/windows-x86_64.json
```

That Python report remains an I/O-only baseline. The Rust report
`docs/reports/mmap-lifecycle-windows-x86_64-2026-07-27.json` is the actual ORT
session experiment; neither report is used to claim production gains.

Run the session experiment with:

```powershell
cargo run -p latexsnipper-runtime --example mmap_lifecycle `
  --features memory-map-model-experimental -- `
  crates/wasm/tests/fixtures/tiny-text-rec.onnx `
  target/runtime-quality/mmap-lifecycle.json
```
