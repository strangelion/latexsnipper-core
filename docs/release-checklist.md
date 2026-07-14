# v2.0.0-rc.1 release checklist

## RC-ready requirements

- [ ] Local fmt, check, strict Clippy, workspace tests, doc tests, WASM builds, and
      TypeScript tests pass.
- [ ] PR CI and manually dispatched Scheduled hardening workflow pass.
- [ ] Chrome and Firefox browser tests pass and diagnostics artifacts are inspected.
- [ ] Trusted plugin soft timeout/quarantine and isolated-process hard timeout pass.
- [ ] Capability projection and drift tests pass.
- [ ] Official production-derived model executes in Tract/WASM with verified origin,
      license, checksum, shape, timing, and memory report.
- [ ] Dependency audit, real libFuzzer smoke, model URL verification, and benchmark
      artifacts pass.
- [ ] Known fidelity and unsupported capabilities are in release notes.

## GA blockers

- [ ] Implement and validate a WASI Component host before advertising execution of
      untrusted third-party plugins.
- [ ] Complete registry/signature/provenance/update policy before remote plugin install.
- [ ] Obtain production OCR-model compatibility and accuracy evidence beyond the
      document-orientation compatibility smoke.
- [ ] Verify live GitHub ruleset/CODEOWNERS approval requirements.
- [ ] Define supported fidelity guarantees per Office/PDF corpus and platform.

## Optional future enhancements

- [ ] Stable native dynamic-library C ABI if process IPC is insufficient.
- [ ] Longer nightly fuzzing and benchmark trend storage.
- [ ] More browser engines and mobile memory profiles.
- [ ] Additional advanced CLI options when underlying importers/exporters support them.
