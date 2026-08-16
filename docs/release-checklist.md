# Core 3 release checklist

## Unpublished integration requirements

- [x] The workspace and WASM package use one internal development version.
- [x] Public API inventory, version map, breaking changes, migration guide, and compatibility policy are maintained.
- [x] Plugin/model migrations reject unsafe reinterpretation and emit structured manual-action warnings.
- [x] New schema and migration fuzz targets compile in PR CI.
- [x] Documentation separates implemented behavior, compatibility adapters, and planned work.
- [x] Stable-tag release guard rejects every prerelease version and manual publication path.
- [x] Lockfiles, frozen contract/source-tree hashes, and version consistency are CI-gated.
- [ ] Every stacked integration PR passes the full workspace, all-feature, documentation, WASM/TypeScript, dependency-audit, actionlint, and fuzz-build gates.

## GA runtime and evidence requirements

- [ ] Local fmt, check, strict Clippy, workspace tests, doc tests, WASM builds, and
      TypeScript tests pass.
- [ ] PR CI and manually dispatched Scheduled hardening workflow pass.
- [ ] Chrome and Firefox browser tests pass and diagnostics artifacts are inspected.
- [x] Trusted plugin soft timeout/quarantine and isolated-process hard timeout pass.
- [x] WASI Component real fixtures cover hard timeout, in-flight cancellation,
      default-deny brokers, memory/output limits, cleanup, and host reuse.
- [x] Capability projection and drift tests pass.
- [ ] Official production-derived model executes in Tract/WASM with verified origin,
      license, checksum, shape, timing, and memory report.
- [ ] Dependency audit, real libFuzzer smoke, model URL verification, and benchmark
      artifacts pass.
- [x] Known fidelity and unsupported capabilities are in release notes.
- [x] Office/PDF golden corpora, six-dimensional evidence, diagnostic, asset,
      opaque-part, and optional visual/application layers run in CI.

## GA blockers

- [x] Integrate signed registry metadata, verified disabled remote installation,
      signature/provenance/update policy, and the public management CLI.
- [x] Integrate verified remote packages with public WASI execution, including
      enable-state checks and host-side package re-verification.
- [x] Obtain an independent security review before advertising third-party execution.
      See [SECURITY_REVIEW.md](../../SECURITY_REVIEW.md) for the full audit report.
- [x] Release owner explicitly approves the `RUSTSEC-2026-0009` (`time`) audit
      exception before shipping. The exception is scoped to a `tract-linalg`
      build dependency whose vulnerable parser is never invoked by tract's code
      generator and is absent from shipped runtime trees; it is accepted pending
      a compatible tract release with `time >= 0.3.47`. Reject or re-evaluate if
      `time` enters a shipped runtime dependency tree or the exception is widened
      beyond the exact advisory ID.
      See [security-review.md](v3/security-review.md).
- [ ] Obtain production OCR-model compatibility and accuracy evidence beyond the
      document-orientation compatibility smoke.
      See [model-evidence.md](model-evidence.md) for the evidence template and
      [governance-verification.md](governance-verification.md) for execution steps.
- [ ] Verify live GitHub ruleset/CODEOWNERS approval requirements.
      See [governance-verification.md](governance-verification.md) section 6.
- [ ] Define supported fidelity guarantees per Office/PDF corpus and platform.
  - [x] Core registry and repository corpus guarantees are generated and gated.
  - [ ] Execute and approve platform-specific Microsoft Office/viewer visual smoke.
      See [visual-smoke-checklist.md](visual-smoke-checklist.md) for the test matrix.

## Pre-release documents

| Document | Purpose | Status |
| ---------- | --------- | -------- |
| [SECURITY_REVIEW.md](../../SECURITY_REVIEW.md) | Independent security audit of plugin/WASI system | [x] Complete |
| [model-evidence.md](model-evidence.md) | Model accuracy/runtime evidence template | [ ] Requires evaluation run |
| [visual-smoke-checklist.md](visual-smoke-checklist.md) | Manual Office/PDF visual smoke test matrix | [ ] Requires manual execution |
| [governance-verification.md](governance-verification.md) | Final pre-release governance checks | [ ] Requires CI execution |

## Optional future enhancements

- [ ] Stable native dynamic-library C ABI if process IPC is insufficient.
- [ ] Longer nightly fuzzing and benchmark trend storage.
- [ ] More browser engines and mobile memory profiles.
- [ ] Additional advanced CLI options when underlying importers/exporters support them.
