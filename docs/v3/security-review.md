# Core 3 security review record

## Scope

The release security scope includes WASI Component verification/execution,
manifest-to-runtime capability binding, signed registry metadata and transport,
archive extraction, remote store state transitions, native process containment,
model downloads, parser limits, migrations, atomic output, and release supply
chain automation.

Repository tests cover malicious Components, traps, invalid patches, broker
denial, memory/output/fuel/deadline limits, cancellation, host reuse, package
replacement, revocation, rollback/freeze, registry thresholds, redirect/MIME/
size policy, ZIP traversal/symlink/duplicate/ratio limits, process-tree timeout,
and crash-resistant replacement.

Pull requests also run locked Rust/npm dependency audits and CodeQL
`security-extended` analysis for Rust and JavaScript/TypeScript. These automated
scans supplement but do not replace the independent final-commit approval.

Filesystem brokers use directory handles, handle-relative operations, and
no-follow opens. Unix native process plugins run in a dedicated session/process
group; Windows uses a kill-on-close Job Object. Native code is still not an OS
filesystem/network sandbox.

## Dependency audit exceptions

`cargo audit --deny warnings` is enforced with the following reviewed
unmaintained-only exceptions. None is a known vulnerability at this review.

| Advisory | Dependency path | Rationale and disposition |
|---|---|---|
| RUSTSEC-2024-0436 (`paste`) | `image -> ravif -> rav1e` and `imageproc -> nalgebra -> simba` | Compile-time macro dependency; monitor the image stack and remove when upstream does. |
| RUSTSEC-2026-0192 (`ttf-parser`) | `resvg/usvg` and `imageproc/ab_glyph` | Used for bounded font/SVG processing; no maintained compatible replacement is currently integrated. Re-evaluate upstream before GA. |
| RUSTSEC-2026-0206 (`rustybuzz`) | `resvg -> usvg` | Used for text shaping in bounded rendering; track the resvg/usvg replacement path. |
| RUSTSEC-2026-0009 (`time`) | `tract-linalg` build dependency only | The vulnerable parser is not called by tract's code generator and `time` is absent from shipped runtime dependency trees. Tract 0.22 breaks the verified production TrOCR model; remove this exception when a compatible tract release accepts `time >=0.3.47`. |

Exceptions are exact advisory IDs, never wildcard categories. A vulnerability,
unsoundness advisory, or changed dependency path blocks release until separately
reviewed and fixed or explicitly accepted with release-owner approval.

RUSTSEC-2026-0217 is fixed without an exception by the vendored
`tract-nnef` 0.21.16 security backport. It applies upstream commit `34c7df2`
to the production-compatible 0.21.10 graph stack and includes a regression test
that rejects overflowing attacker-controlled tensor dimensions. The remaining
tract packages are pinned to 0.21.10 because the upstream 0.21.16 graph
translator rejects the frozen production decoder model; the exact production
profile is exercised in CI.

## Independent review status

This document records internal implementation review and automated evidence. It
is not an independent security approval. GA advertising of third-party plugin
execution remains blocked until a reviewer independent of the implementation
approves the final security-sensitive commit, with stale approval dismissal and
required checks enforced by the live `main` ruleset.

Reviewers should start with
[plugin-registry-threat-model.md](plugin-registry-threat-model.md),
[wasi-component-host.md](wasi-component-host.md), and the frozen contract
manifest at `contracts/v3-contract-freeze.json`.
