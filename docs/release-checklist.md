# Core 3 release checklist

## Alpha 1 contract-foundation requirements

- [ ] Every workspace crate and the WASM npm package uses `3.0.0-alpha.1`.
- [ ] Public API inventory, version map, breaking changes, migration guide, and compatibility policy are reviewed.
- [ ] Plugin/model migrations reject unsafe reinterpretation and emit structured manual-action warnings.
- [ ] New schema and migration fuzz targets compile in PR CI.
- [ ] Documentation clearly separates implemented, contract-only, existing v2 runtime, and planned behavior.
- [ ] Full workspace, all-feature, doc, WASM/TypeScript, dependency-audit, actionlint, and fuzz-build gates pass.

Alpha 1 is not an RC and must not satisfy later runtime/security items by checking only a contract type or stub.

## RC-ready requirements

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

## GA blockers

- [x] Integrate signed registry metadata, verified disabled remote installation,
      signature/provenance/update policy, and the public management CLI.
- [ ] Integrate verified remote packages with public WASI execution and obtain an
      independent security review before advertising third-party execution.
- [ ] Obtain production OCR-model compatibility and accuracy evidence beyond the
      document-orientation compatibility smoke.
- [ ] Verify live GitHub ruleset/CODEOWNERS approval requirements.
- [ ] Define supported fidelity guarantees per Office/PDF corpus and platform.

## Optional future enhancements

- [ ] Stable native dynamic-library C ABI if process IPC is insufficient.
- [ ] Longer nightly fuzzing and benchmark trend storage.
- [ ] More browser engines and mobile memory profiles.
- [ ] Additional advanced CLI options when underlying importers/exporters support them.
