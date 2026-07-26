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

Model mmap is not enabled in production. An isolated
`runtime-mmap-experimental` example now maps a versioned model and uses ORT's
borrowed `commit_from_memory_directly` API, which makes the map outlive the
session at the Rust type level. It runs first/warm inference, atomically
switches a version pointer, proves the old session remains usable, loads the new
version, records process memory and Windows delete/replace behavior, and checks
temporary-directory cleanup. Accordingly:

- `model-loading:mmap` is not advertised;
- `memoryMapModel: true` fails with `ONNX_MODEL_MMAP_UNSUPPORTED`;
- no RSS or cold-start improvement is attributed to mmap.

The experiment is positive lifecycle evidence, but not proof of a production
RSS or cold-start improvement. Keeping the advertised capability disabled
prevents an option from being silently accepted before the cache/session owner
implements the same lifetime contract.

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
  --features runtime-mmap-experimental -- `
  crates/wasm/tests/fixtures/tiny-text-rec.onnx `
  target/runtime-quality/mmap-lifecycle.json
```
