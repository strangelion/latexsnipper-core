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

`cargo audit --deny warnings` is enforced with the following exact reviewed
exceptions: three maintenance warnings and one build-only parser advisory.
The latter is not a general exception for vulnerable runtime dependencies.

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

The WASI host pins Wasmtime 36.0.16 for RUSTSEC-2026-0316. Liquid's private
register backend uses the compatible `anymap3` maintenance backport documented
in `vendor/liquid-core-0.26.8-anymap3/BACKPORT.md`, removing `anymap2` and
RUSTSEC-2026-0319 without changing the production Tract graph versions.
Neither change broadens the audit exceptions. Pipeline YAML now uses a pinned,
deserialize-only `serde-saphyr` profile, removing `serde_yaml` and
`unsafe-libyaml` (RUSTSEC-2024-0449) from the locked graph. Its input, parser,
anchor and replay budgets are documented in [the pipeline guide](../pipeline.md#yaml-清单).
File includes and environment interpolation are not enabled; duplicate keys and
unsupported tags are rejected, while legacy literal merge keys are preserved.

CI invokes `scripts/verify_dependency_audit.py`, which runs the normal
`cargo audit --deny warnings --json` command without skipping database refresh or
yanked-package checks. A valid clean JSON report is insufficient if the auditor
also reports execution/registry errors on stderr, even when it exits zero.
Malformed reports and incomplete checks fail the gate. Exact existing exceptions
remain unchanged; diagnostic advisory-only checks are not complete audit evidence.

## Host approval gate and remaining integration

C-09 prioritizes Core/WASI sensitive broker actions. Existing manifest grants,
package enablement and path/network bounds are not per-action human approval.
The Rust WASI host now gates writes and external network sends with a default-deny,
nonblocking controller. Application dialogs, durable user policy and independent
approval are not completed by this implementation.

Remembered-policy metadata has a bounded HMAC-authenticated snapshot route
and exact latest-checkpoint validation. Trusted opt-in hosts now issue fresh
bounded one-action permits from a live validated shared store. These primitives
do not restore session grants, persist files or integrate an OS key store. Replay protection
depends on a separately protected latest store/revision/digest/time checkpoint;
a MAC alone cannot prevent replay of both snapshot and checkpoint. Removing
stored metadata revokes its shared per-use authority but not separately issued
explicit session grants. No stored scope/deadline is automatically extended.
Crash-safe host persistence, trusted UI events and independent security approval
remain required for application-level reuse. See the [host contract](wasi-component-host.md#opt-in-remembered-execution-and-live-revocation).

Live authoritative-owner guard hooks now bind complete per-policy identities at
issuance/publication/dispatch checkpoints. Native checks run outside Core mutexes;
their deadline is clipped to the earlier guest/use deadline and a 50 ms check
budget, with late success rejected and cancellation rechecked before effects.
These hooks do not implement a protected owner service, OS key store, atomic
durable publication or real multi-process coordination. A native provider remains
trusted code and must not wait for UI or block without its own timeout. See the
[guard integration contract](wasi-component-host.md#live-authoritative-policy-guard).

The host, never the guest, must own approval decisions for sensitive writes,
overwrites and external data transfer. Offer explicit one-action, whole-batch
and bounded-session approval, with visible scope, expiry and revocation.
Actions within a current explicit grant should proceed without repeated prompts;
changed package identity, out-of-scope targets/actions or expired grants require
new consent. A temporary refusal must not silently become a permanent block.
Missing approval prevents that action, but is not a rollback guarantee for earlier
approved actions. Confirmation never waits inside the broker; guest budgets and
manifest/path bounds remain independent. The API and limitations are documented in
[the host contract](wasi-component-host.md#host-user-approval).
Single-file writes stage before publication and recheck observed content, retaining
original bytes on staging/cancellation/rename failure. Per-use permits recheck
consent expiry/revocation at staging/publication and pre-network-dispatch checkpoints;
exhausted started uses remain revocable without re-prompting valid actions. Closing
or changing a host discards its stale requests/grants. This does not provide
external-writer CAS, whole-invocation rollback or crash recovery. Windows staging
now preserves and verifies owner/group/DACL before payload writes and rechecks that
security identity before publication. Unix ownership and other extended metadata,
Windows SACL/security resource attributes and encryption/sparse/integrity semantics
are still open. Storage preflight refuses existing encrypted, sparse, reparse or
integrity targets with `PLUGIN_WASI_METADATA_UNSUPPORTED` before requesting approval,
unless the no-follow path policy has already refused the target. This refusal is
not preservation support. Controlled sparse-file evidence covers
this path; EFS flag checking is not a full EFS/certificate test. NTFS primary and
named-stream NONE/LZNT1 compression is preserved on staging and rechecked before
publication; compression-only changes are conflicts. Bounded named `$DATA` streams
are copied to owned staging files,
checkpointed and rechecked on both original and candidate; stream-only edits refuse
publication, and unknown/oversize metadata is not silently dropped. Enumeration,
names, record count and aggregate bytes have independent limits. Stream creation
cannot implicitly create a missing base or overwrite an existing stream. Controlled
x64 evidence is not every-filesystem, remote-share or enterprise-policy
approval. No process privileges or global machine settings are enabled to bypass
preservation failures. These boundaries remain open for independent review.

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
