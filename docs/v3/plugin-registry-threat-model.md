# Core 3 signed plugin registry threat model

## Scope and security objective

The signed registry distributes third-party `WasiComponent` packages. Its objective is to
preserve an explicitly trusted namespace-to-digest binding despite a malicious registry,
mirror, network, or package. Remote installation never grants trust to native code. Reviewed
local native-process packages remain a separate, explicit, lower-trust workflow.

TLS protects transport to a configured origin; Ed25519 thresholds over canonical metadata
establish plugin identity, version, execution class, compatibility, size, and SHA-256.

## Trust anchors and canonical signatures

An operator adds an HTTPS origin in the `unverified` state, then explicitly trusts a local root
metadata file. `snipper plugin registry trust` requires `--yes`. The first root is self-signed
at its configured threshold. A replacement root must be exactly the next version and satisfy
both the old and new root thresholds. Same-version replacement with different bytes is rejected.

All registry signatures use Ed25519. A key is identified by an explicit key ID, and duplicate
signatures from one key count once. The signed bytes are compact UTF-8 JSON of a strongly typed
schema with fixed struct field order, ordered `BTreeMap`/`BTreeSet` collections, no floating-point
values, no arbitrary extension values, and no signature array. Unknown schema fields are rejected.
Serialization stability has a fixed-byte regression test.

```text
root -> timestamp -> snapshot -> targets -> package SHA-256/size/class/compatibility
```

Each role has independent key IDs and thresholds. Timestamp binds the exact snapshot envelope;
snapshot binds the exact targets envelope. Length and SHA-256 are checked before the next envelope
is accepted. Every role has a monotonic version and an absolute expiry.

## Attacker analysis

| Threat | Control | Residual limitation |
|---|---|---|
| Malicious registry or compromised mirror | Cannot create a valid metadata threshold; target metadata pins ID, version, class, size, digest, and compatibility | A threshold key compromise remains authoritative until rotation or revocation |
| TLS interception | HTTPS with the platform trust store, configured origins only, no arbitrary URL installs | A compromised public CA can interrupt traffic but cannot forge signed metadata |
| Digest or signature substitution | Ed25519 verification over canonical bytes plus exact SHA-256 and length | SHA-256 and Ed25519 are fixed by schema v1 |
| Key compromise and rotation | Per-role keys, thresholds, explicit IDs, dual-threshold sequential root rotation | Emergency replacement still needs the current root threshold |
| Rollback attack | Persisted minimum versions; root updates increment exactly once | Deleting all local trust state is an explicit operator reset |
| Freeze or stale metadata | Absolute expiry; cached metadata is reverified before search/install | Offline refresh fails after expiry; installed bytes remain inspectable |
| Plugin ID takeover or dependency confusion | Signed map key must equal plugin ID; installs select an exact ID from trusted caches | Registry governance controls initial ID ownership |
| Redirect abuse | Three redirects maximum; exact HTTPS scheme/host/effective-port match; credentials forbidden | Same-origin paths remain controlled by the origin |
| Content-type confusion | Separate metadata/package MIME allowlists; encoded responses rejected | Servers must emit documented MIME types |
| Archive bomb | Compressed, decompressed, file-count, and per-file limits with checked arithmetic | Limits do not describe later runtime memory |
| Archive traversal or symlink | Normal relative forward-slash paths only; no duplicate, parent, drive, absolute, symlink, or special file | Filesystem ACLs remain an OS responsibility |
| Interrupted install | Cross-process lock, same-filesystem staging, file flush, durable index replacement, staging cleanup | Package directory and JSON index are not claimed as one atomic transaction |
| Corrupt local state | Last-known-good index backup, bounded strict JSON, consistency doctor, quarantine | Write access to trust and store state permits denial of service |
| Revoked or malicious update | Signed revocation rejection, local disable/revoke, version downgrade rejection | Revocation must arrive before metadata expiry |
| Execution-class downgrade or native substitution | Target, manifest, artifact kind, and validated Wasm Component binary must agree | Reviewed local native-process install remains separate |

## Downloader and package boundary

The downloader sends `Accept-Encoding: identity`, applies a 20-second request/connect timeout,
limits redirects to three, and reads through a `maximum + 1` guard even with dishonest or absent
Content-Length. Metadata is limited to 8 MiB per envelope and packages to 64 MiB compressed.
Extraction defaults to 128 MiB total, 64 MiB per file, and 256 members.

Verification occurs while extracting to private staging and again from staged filesystem objects.
The package must contain `plugin.json` with manifest schema 3. Its ID/version/class must match the signed target, its
artifact digest and size must match the manifest, and `wasmparser` must validate the artifact as a
Component. No plugin code runs during download, install, update, rollback, verification, or doctor.

## Storage, recovery, and offline behavior

Remote packages use a store separate from legacy local packages. Versions are immutable directories
under `remote/packages/<id>/<version>`. A schema-versioned index records active and last-known-good
versions. Writers take an OS-backed lock. Files are flushed before same-volume rename; Unix syncs
directories and Windows uses `MoveFileExW` replace/write-through for index activation. The previous
index is flushed before replacement.

If activation stops after directory rename but before index replacement, doctor reports the orphan.
Missing or invalid indexed directories are quarantined and disabled. Leftover staging directories
are removed under lock. Rollback changes the active index pointer only after the last-known-good
directory is present.

Offline refresh revalidates the complete cached chain, expiry, and persisted minimum versions. An
installed verified package can be checked offline from stored provenance; expired cache never permits
a new install.

## Trust-state vocabulary and commands

CLI JSON distinguishes `trusted_built_in`, `reviewed_local_native_process`,
`verified_wasi_component`, `unverified`, `expired`, `revoked`, `quarantined`, and `incompatible`.
Remote install creates a disabled `verified_wasi_component`; execution remains a separate hardened
WASI-host decision.

```text
snipper plugin registry list
snipper plugin registry add <name> <https-origin>
snipper plugin registry remove <name> --yes
snipper plugin registry trust <name> <root.json> --yes
snipper plugin registry refresh [name] [--offline]
snipper plugin search [query]
snipper plugin install <id>
snipper plugin update <id> | --all
snipper plugin rollback <id>
snipper plugin verify <package-or-id>
snipper plugin info <id>
snipper plugin doctor
snipper plugin revoke <id>
```

Arbitrary URL installation is deliberately unsupported.

## Validation evidence

Tests cover canonical serialization, real Ed25519, thresholds, malformed/unknown keys, root rotation,
expiry, rollback, remote-class policy, digest mismatch, redirect origin, content type, size limits,
traversal, symlink, malformed Component bytes, staging re-verification, update, rollback, interrupted
staging, corrupt-index recovery, concurrent install, revocation, offline cache, HTTP rejection,
arbitrary URL rejection, trust confirmation, registry-to-host activation binding, and post-activation
disable/revocation enforcement. Signed-envelope fuzzing and the existing WASI package fuzz target
must pass before the single `3.0.0` GA release.
