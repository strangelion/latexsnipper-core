# WASI Component host v1

`latexsnipper-plugin-wasi` executes Component Model packages described by a
plugin manifest v3. Its stable guest contract is
`wit/plugin-v1` (`latexsnipper:plugin@1.0.0`); this version is independent of
the Rust crate and manifest schema versions.

## Authority model

The linker does not add WASI CLI, filesystem, sockets, environment, stdio, or
process interfaces. A guest can use only these typed brokers:

- execution cancellation and remaining deadline;
- grant-ID plus relative-path filesystem reads and writes;
- allowlisted environment values;
- exact scheme/host/port network destinations and path-only requests;
- named model artifacts;
- bounded in-memory temporary files;
- separately granted monotonic clock and secure randomness.

Unknown imports fail instantiation. Paths must be relative and are opened from
capability directory handles with no-follow semantics on Linux, macOS, and
Windows. Traversal, absolute/prefix bypass, symlink escape, non-regular files,
and hard-link aliases are rejected. The host repeats the artifact digest check
through the verified package directory handle immediately before compilation.
The broker never resolves an untrusted relative path into an ambient host path.

Runtime capability declarations must exactly match the verified manifest.
Execution entrypoints additionally require the matching registration grant,
hook, and importer/exporter format declaration. Broker calls require both a
runtime declaration and a concrete manifest permission; only actually granted
capabilities are included in the initialization context.

## Host user approval

Manifest grants and package enablement no longer suffice for filesystem writes
or any external network send (including GET/query data). These brokers also
require the host-owned `SensitiveActionController`. A missing grant stops the
guest before that action and returns `PLUGIN_WASI_APPROVAL_REQUIRED`; the host
can read `approval_controller().pending()` without running a UI callback inside
the guest deadline. It must display the action and obtain a trusted user event
before calling `approve_once`, `approve_batch`, or `approve`. Never expose these
approval methods as automatically callable guest/MCP tools.
Render targets and metadata as escaped plain text, not guest HTML or a plugin's
generated explanation; bind the confirmation to the controller's actual request ID.

Requests bind plugin ID/version, component SHA-256, verified manifest, the host
instance/context, operation and target. File actions include the capability root,
relative path, new payload hash/length and the observed old file hash (or absence).
Network actions include exact scheme/host/port/method/path/query and body hash/length;
payload bytes and environment secrets are not copied into the request. Paths and
URLs themselves may be sensitive: do not log or upload request snapshots by default.
Changing configuration, injected environment/model artifacts or the network broker
invalidates the host context. A new activation cannot reuse old consent, even for
the same package version. Remote activation still rechecks enablement/trust.

`Exact` authorizes that complete action; `SameTarget { max_payload_bytes }` is an
explicit, broader user choice permitting changed payload/old file content for the
same file or exact network route. No directory/origin wildcard is supported. Each
grant is limited to 1..10000 uses and a positive lifetime of at most one hour.
`approve_once` allows one use for five minutes; `approve_batch` atomically confirms
a displayed list of pending exact requests, once each. Valid in-scope uses proceed
without additional prompts. Exhaustion, expiry, target changes or context changes
require fresh confirmation. `revoke`/`clear` remove future consent and pending
requests and invalidate live per-use permits, not a permanent blacklist.
Consumption is synchronized across callers;
unknown IDs and over-budget/invalid batches fail closed.
Each use is consumed before IO, even if that IO subsequently fails or is cancelled;
there is no automatic retry or quota refund.
An exhausted grant stays revocable while its started use still holds a permit;
`grants()` includes `inFlightUses` for that current grant even with zero remaining
uses. Exhaustion itself does not stop an already-approved use. Renewing/replacing
consent invalidates the old permits and creates a fresh grant; it never silently
extends a started use. Context changes, switching controllers and host destruction
discard only that host context's requests/grants, preserving other live hosts on
a shared controller. Observing a poisoned controller lock also invalidates its
recorded permits and fails closed.

This is an in-memory Rust host interface, not an application dialog or a durable
user policy store. Controllers are available from `WasiComponentHost`,
`ActivatedRemoteWasiPlugin` and `PreparedFormulaRegistry`. The formula CLI does not
auto-approve or add an interactive retry; registered conversion failures remain
terminal. A user-confirmed fresh invocation is distinct from automatic fallback.
Approval does not broaden manifest permissions, bypass path/output/guest budgets,
or sandbox a trusted native broker. No process/installer/registry/certificate broker
is added. Temporary in-memory guest files are not external writes.

File writes now stage bounded chunks in an exclusively created random file in the
target's parent directory, sync and close it, then replace the target only after
checking its observed hash/absence again. Directory traversal rejects symlink
parents; regular-file, hardlink and size checks still apply. Cancellation/deadline
checkpoints run during staging, while waiting for publication and immediately
before replacement. Clones of one grant share publication ordering; a detected
content change returns host-only `PLUGIN_WASI_WRITE_CONFLICT`, never an automatic
retry. Its existing WIT mapping remains `host-failure`; WIT v1 is unchanged.
Staging is removed on normal error/cancellation paths, not by deleting the original.
The consumed consent use is not refunded, and a valid session grant remains usable
for a fresh user-directed operation. Successful publication does not remove a later
file created at the former staging name.
Every staging/publication checkpoint also rechecks that use's consent expiry and
revocation flag. Host-only `PLUGIN_WASI_APPROVAL_REVOKED` and
`PLUGIN_WASI_APPROVAL_EXPIRED` stop execution and preserve the original file before
replacement; both retain the existing WIT `permission-denied` mapping. No checkpoint
opens a dialog or consumes another consent use. Network sends have a final consent
checkpoint before dispatch to the native broker. Revocation is cooperative: a
native replacement/send already entered after the last checkpoint cannot be
undone, and the last check plus OS call is not atomic with controller revocation.
Already-dispatched native network IO is not forcibly cancelled or retried.

Approval is per action, not a transaction for a whole invocation. Previously
published writes cannot be rolled back by a later denial or revocation. The old
file digest is an observed precondition, not an atomic filesystem compare-and-swap:
external/native writers can race between the last check and replacement, and
independently created grant/host instances do not share a global filesystem lock.
On Windows the pinned capability library's rename ultimately uses native paths;
this is not a guarantee against hostile native directory moves/reparse changes.
Replacement preserves ordinary file permission bits (never Unix setuid/setgid),
not owner identity, platform-specific ACLs, alternate streams or other extended
metadata. Grants remain package-relative; do not use this implementation as a
general-purpose editor for external security-sensitive files. Platform metadata
preservation, multi-file transactions and crash recovery remain separate work.
An abrupt host exit can leave a random staging file; there is no crash journal or
power-loss durability guarantee. Cleanup is best-effort if the OS refuses removal.
Synchronous native network/IO implementations must still enforce their own deadlines. The
approval check itself never waits for human input or executes external callbacks.
Network paths reject authority-like `//`, backslashes, fragments and control
characters. Methods are bounded HTTP tokens, preserving valid case/custom methods.
Native brokers must not follow redirects or reinterpret paths into a different,
unapproved destination; approval is not a network sandbox for native code.

## Execution and cleanup

Every invocation receives a fresh Store, ResourceTable, and component instance.
Fuel supplies a deterministic CPU budget. A single 5 ms epoch ticker belongs to
the host, while each Store has its own callback that checks only that execution's
deadline and cancellation token. One timeout therefore cannot interrupt another
concurrent Store. Component calls, broker calls, output validation, and shutdown
must complete before output is published; any failure drops the Store and all
guest resources.

Manifest limits are requests, not authority. The host applies explicit
minimum/default/maximum policy, rejects zero, below-minimum, and integer-width
overflow values, and clamps requests to host ceilings. Limits cover memory,
tables, memories, core instances, resources, input, output, diagnostics, model
bytes, temporary bytes, fuel, deadline, and concurrent executions. Waiting for
a concurrency permit is itself cancellable and included in the deadline.

Guest errors from initialize, invocation, and shutdown retain bounded structured
diagnostics. Shutdown runs after every successfully initialized invocation and a
shutdown failure prevents otherwise successful output from being published.

## Package boundary

The host accepts only unpacked directories containing `plugin.json` and the
declared component artifact. Traversal is deterministic and bounded by entry,
file, directory, recursion-depth, total-byte, metadata-byte, path-length, and
wall-clock limits. It validates manifest/core/API/WIT compatibility, semantic
versions, artifact kind/path/size/SHA-256, license, configuration-schema shape,
normalized path uniqueness, file/link types, declared payload roots, and
signature metadata shape. Archives and compression are rejected rather than
extracted.

Cryptographic signature/provenance verification, rollback/freeze protection,
bounded archive extraction, and disabled remote installation are implemented by
the signed registry layer. The Component host remains the only permitted execution
boundary; remote packages are not enabled or executed during installation. See the
[registry threat model](plugin-registry-threat-model.md).

## Stable diagnostics

- `PLUGIN_WASI_TRAP`
- `PLUGIN_WASI_TIMEOUT`
- `PLUGIN_WASI_CANCELLED`
- `PLUGIN_WASI_MEMORY_LIMIT`
- `PLUGIN_WASI_OUTPUT_LIMIT`
- `PLUGIN_WASI_RESOURCE_POLICY`
- `PLUGIN_WASI_PERMISSION_DENIED`
- `PLUGIN_WASI_APPROVAL_REQUIRED` (host-only; WIT v1 remains unchanged)
- `PLUGIN_WASI_CAPABILITY_MISMATCH`
- `PLUGIN_WASI_INVOCATION_NOT_DECLARED`
- `PLUGIN_WASI_PROTOCOL_MISMATCH`
- `PLUGIN_WASI_INVALID_INPUT`
- `PLUGIN_WASI_INVALID_PATCH`
- `PLUGIN_WASI_HOST_FAILURE`
