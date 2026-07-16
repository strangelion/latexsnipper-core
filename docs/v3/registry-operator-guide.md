# Signed registry operator guide

Core 3 consumes TUF-style root, timestamp, snapshot, and targets metadata for
remote WASI Component packages. Registry operation is a security role, not a
file-hosting shortcut.

## Authority and separation

- Keep root keys offline. Root rotation requires the old and new configured
  thresholds.
- Use separate online keys for timestamp, snapshot, and targets roles.
- Serve metadata and immutable package archives over HTTPS with identity
  encoding. The client rejects cross-origin redirects, oversized responses,
  unexpected MIME types, rollback, freeze, expiry, and digest mismatches.
- Never distribute native executables through the remote registry. Core 3
  accepts only manifest-v3 `wasi_component` targets.

The repository intentionally does not contain production private keys or a
production signing ceremony. Operators must use an audited offline signing
system and retain ceremony records outside the public repository.

## Publication order

1. Build the Component package from a reviewed source commit.
2. Verify `plugin.json`, `component.wasm`, license, provenance, file count,
   compressed/decompressed size, and SHA-256 locally.
3. Add the immutable target and its exact length/digest to targets metadata.
4. Increment and sign targets metadata.
5. Increment snapshot metadata and bind the exact targets metadata version and
   digest.
6. Increment timestamp metadata and bind the exact snapshot metadata.
7. Publish the package and versioned metadata before atomically replacing the
   timestamp entry point.
8. Test search, install, verify, explicit enable, invocation, disable, revoke,
   and rollback from a clean client store.

Never reuse a version counter or replace bytes behind an already published
digest. A correction is a new target and new metadata version.

## Revocation and recovery

For a compromised or unsafe target, publish a new targets version that marks
the exact plugin version revoked, then update snapshot and timestamp metadata.
Clients re-check active version, enable state, revocation, manifest identity,
artifact digest, package provenance, and the bound registry snapshot before
every remote invocation.

If an online role key is compromised, rotate it with root-authorized metadata.
If the root threshold is compromised, stop publication and perform the
documented offline recovery ceremony. Do not lower thresholds to restore
availability.

## Operator evidence

Retain the source commit, tool versions, unsigned canonical payloads, signatures,
key IDs, role thresholds, final metadata, package digest, publication time, and
clean-client verification output. Do not store private keys, tokens, internal
hostnames, or operator personal information in this repository.

The consumer threat model is defined in
[plugin-registry-threat-model.md](plugin-registry-threat-model.md).
