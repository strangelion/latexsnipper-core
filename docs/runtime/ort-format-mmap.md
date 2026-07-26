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

Model mmap is not enabled. The current Rust ORT binding accepts a filesystem
path but does not expose a lifecycle contract that proves the model graph is
read-only mapped, controls Windows share/delete flags, or retains the mapping
for the session lifetime. Accordingly:

- `model-loading:mmap` is not advertised;
- `memoryMapModel: true` fails with `ONNX_MODEL_MMAP_UNSUPPORTED`;
- no RSS or cold-start improvement is attributed to mmap.

This prevents an option from being silently accepted without evidence.

## Update and locking design

Future mmap enablement must keep the mapping in a session-owned `Arc`, close it
only after the last task releases the old session, and install models through a
validated staging directory. The installer must verify archive limits,
package-relative paths, schema version, and SHA-256 before an atomic rename.
On Windows it must explicitly test replacement while the old file is mapped
and fall back to versioned directories plus an atomic pointer switch.

The existing model importer already enforces checksums, traversal/symlink
rejection, archive count/size/ratio budgets, staging cleanup, and executable
artifact exclusion. True live version switching remains future work.

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

The report is deliberately marked `io_only_not_ort_session`. It is a capability
and operating-system baseline, not evidence of ONNX Runtime startup gains.
