# Core 3.0.0 release notes (GA draft)

This file is the source draft for the single `3.0.0` GA release. No alpha, beta,
or RC package or tag is published.

## Highlights

- Versioned v3 API envelopes, capability projection, migrations, CLI behavior,
  TypeScript declarations, and additive FFI metadata.
- Default-deny WASI Component host with verified manifest binding, hard timeout,
  cancellation, resource ceilings, bounded brokers, and real malicious fixtures.
- Signed registry metadata, bounded HTTPS and ZIP handling, disabled-by-default
  remote installation, revocation, rollback, atomic store updates, and
  re-verification before every remote invocation.
- Native and browser OCR pipelines with table and handwriting profiles,
  reproducible evidence schemas, production-derived runtime compatibility, and
  explicit non-production contract fixtures.
- Executable Office/PDF fidelity evidence across six independent dimensions.
- Atomic CLI output and source-preserving v2-to-v3 migration commands with exit
  code 11 for manual action.

## Compatibility

The serialized `Document` schema remains `1.0.0`. Worker protocol v1, browser
cache schema v2, native process IPC v1, and Component WIT v1 evolve independently
from crate version 3.0.0. V2 WASM endpoints remain callable compatibility
adapters. Future schemas are rejected instead of being reinterpreted.

See [migration-from-v2.md](migration-from-v2.md) and
[breaking-changes.md](breaking-changes.md).

## Security boundaries

Remote distribution accepts WASI Components only. Native process plugins are
reviewed local code and are not an operating-system filesystem/network sandbox.
In-process plugin deadlines are cooperative; a timed-out thread is quarantined
but cannot be safely force-killed. Network broker access remains default-deny
unless an embedding application provides a bounded approved transport.

## Known fidelity and model limits

Production model execution proves runtime compatibility, not universal OCR
accuracy. Accuracy claims require licensed frozen release corpora. Office/PDF
package validity does not imply identical layout, visuals, editability, or
round-trip behavior in every application. TATR/SLANet browser structure models,
advanced Office objects, arbitrary PDF encodings, and native dynamic-library
plugins remain experimental or unavailable as documented in the capability
matrix.

Final checksums, source commit, CI runs, artifacts, and review status are added
only after the final GA commit is validated.
