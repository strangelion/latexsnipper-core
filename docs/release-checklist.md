# Core 3 release checklist

## Unpublished integration requirements

- [x] The workspace and WASM package use one internal development version.
- [x] Public API inventory, version map, breaking changes, migration guide, and compatibility policy are maintained.
- [x] Plugin/model migrations reject unsafe reinterpretation and emit structured manual-action warnings.
- [x] New schema and migration fuzz targets compile in PR CI.
- [x] Documentation separates implemented behavior, compatibility adapters, and planned work.
- [ ] Every stacked integration PR passes the full workspace, all-feature, documentation, WASM/TypeScript, dependency-audit, actionlint, and fuzz-build gates.

The development version is not published. No alpha, beta, or RC package or tag is created; the next published package is `3.0.0` GA.

## GA runtime and evidence requirements

- [ ] Local fmt, check, strict Clippy, workspace tests, doc tests, WASM builds, and
      TypeScript tests pass.
- [ ] PR CI and manually dispatched Scheduled hardening workflow pass.
- [ ] Chrome and Firefox browser tests pass and diagnostics artifacts are inspected.
- [ ] Trusted plugin soft timeout/quarantine and isolated-process hard timeout pass.
- [x] WASI Component real fixtures cover hard timeout, in-flight cancellation,
      default-deny brokers, memory/output limits, cleanup, and host reuse.
- [ ] Capability projection and drift tests pass.
- [ ] Official production-derived model executes in Tract/WASM with verified origin,
      license, checksum, shape, timing, and memory report.
- [ ] Dependency audit, real libFuzzer smoke, model URL verification, and benchmark
      artifacts pass.
- [ ] Known fidelity and unsupported capabilities are in release notes.
- [x] Office/PDF golden corpora, six-dimensional evidence, diagnostic, asset,
      opaque-part, and optional visual/application layers run in CI.

## GA blockers

- [x] Integrate signed registry metadata, verified disabled remote installation,
      signature/provenance/update policy, and the public management CLI.
- [x] Integrate verified remote packages with public WASI execution, including
      enable-state checks and host-side package re-verification.
- [ ] Obtain an independent security review before advertising third-party execution.
- [ ] Obtain production OCR-model compatibility and accuracy evidence beyond the
      document-orientation compatibility smoke.
- [ ] Verify live GitHub ruleset/CODEOWNERS approval requirements.
- [ ] Define supported fidelity guarantees per Office/PDF corpus and platform.
  - [x] Core registry and repository corpus guarantees are generated and gated.
  - [ ] Execute and approve platform-specific Microsoft Office/viewer visual smoke.

## Optional future enhancements

- [ ] Stable native dynamic-library C ABI if process IPC is insufficient.
- [ ] Longer nightly fuzzing and benchmark trend storage.
- [ ] More browser engines and mobile memory profiles.
- [ ] Additional advanced CLI options when underlying importers/exporters support them.
