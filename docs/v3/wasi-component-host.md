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
Approving a same-target scope also removes pending descriptions already covered by
that consent, so hosts do not keep showing obsolete prompts. Unrelated targets,
oversized payloads and other plugin/host identities stay pending. Dismissing these
descriptions neither executes queued work nor consumes grant uses; the host must
explicitly resume the intended operation. Authenticated remembered policies have
an opt-in execution route below; crash-safe application persistence and UI remain pending.

### Consent experience required of integrating hosts

These are host integration requirements, not additional implemented Core APIs:

- Offer distinct choices for this action, the displayed batch, and this same
  target for a stated session budget. Do not reduce an explicit batch/session
  choice to repeated `approve_once` calls with a new dialog for each use.
- After a trusted confirmation, dismiss covered prompts and resume only the
  intended work. Pending descriptions are not a replay queue: do not rerun a
  whole invocation that may already have published other effects. No permission
  dialog may block the broker thread or consume a guest's execution deadline.
- A valid grant is a fast path. Show remaining scope/expiry unobtrusively, with
  an accessible revoke action; repeated matching calls must not reopen a dialog.
  Explain the specific changed target, identity, byte limit, expiry or exhausted
  budget when fresh consent is needed. Rejecting or closing a prompt does not
  disable unrelated read-only features or create a permanent plugin blacklist.
- If the host later offers "remember this choice", make it a separate explicit,
  revocable opt-in. Store a narrowly scoped host policy, not serialized Core
  grants or pending IDs. Revalidate the verified package, permissions and host
  configuration before issuing a fresh bounded grant; a changed identity or
  broader target/action requires new consent. Within that remembered scope,
  routine runtime-grant renewal must not ask the same question again. The host
  must never infer remembered permission from a manifest or guest output.

### Stable binding for remembered-policy integration

`WasiComponentHost::with_remembered_approval_binding(trusted_domain,
network_policy_identity)` is an opt-in identity foundation, not a consent decision.
Call it after configuring the host. Both opaque inputs must be nonempty and at
most 4 KiB. They must come from trusted host state, never a guest instruction.
The domain should identify the intended application/profile/document scope.
The broker identity must version its implementation and security-relevant settings;
Core cannot introspect an arbitrary trusted native broker's behavior.

The fingerprint covers Core version/platform, verified manifest/component identity,
resolved filesystem/network permissions, effective limits, injected configuration,
environment values and model contents. Config/environment/model values are hashed
before inclusion. Only the digest is exposed by `remembered_approval_binding()`
and the optional pending-request `rememberedBindingSha256` field. These digests
remain privacy-sensitive metadata and should not be logged or uploaded by default.

Changing injected settings or replacing a broker clears this binding and revokes
the previous context's grants. Rebinding requires recomputing it from the effective
settings. Repeating an unchanged binding does not revoke valid grants. Identical
fresh hosts can have equal stable fingerprints, but their random execution contexts
and request IDs remain distinct: a session grant cannot be restored or replayed
merely by knowing the fingerprint.

Remote integrations can opt in with
`ActivatedRemoteWasiPlugin::with_remembered_approval_binding(trusted_domain)`;
this also binds the installed archive SHA-256 and the route's fixed default-deny
broker identity. Rebinding rechecks enablement/trust, while every execution still
checks that the installed archive and manifest are unchanged. Identical component
bytes in a different archive cannot inherit the binding.
`WasiFormulaProfile::prepare_with_remembered_approval_binding(store, cancellation,
trusted_domain)` additionally binds the complete canonical profile, including its
format validator. `PreparedFormulaRegistry::remembered_binding_sha256` is `None`
on the ordinary preparation path. The bound path still leaves the registry backend
disabled and creates no consent grants. Host scope arguments are not guest/profile
JSON fields, and no new guest authority is introduced.

### Authenticated remembered-policy snapshots

`RememberedApprovalPolicies` now provides bounded policy metadata and authenticated
snapshots, not execution grants. The trusted host must obtain a separate explicit
remember-choice event before calling `SensitiveActionController::remember` with
a current owned pending request. Missing stable bindings or stale/unknown request
IDs refuse the operation. Remembering does not dismiss or replay pending work,
approve a session grant, attach a policy store to the broker, or enable any backend.

A policy uses `Exact` or `SameTarget` with the existing exact file/network target
rules. Its duration is positive whole seconds, at most 30 days, with at most
128 policies, 8 KiB action metadata and a 16 MiB payload ceiling. This duration
describes a reusable consent window, not a restored session-use quota.
Updating a same-target policy replaces its previous ceiling/deadline; it does not
silently retain a broader older row. `policies` exposes display metadata;
`matching_policy` returns metadata only and cannot grant authority. Expired rows
may remain visible but cannot match. `revoke` and `clear` remove policy metadata
and revoke that store's private per-use authority, not independently issued
in-memory session grants or native effects.

Use a dedicated generated `ApprovalPolicyKey`, protect its exported bytes with
the host's key store, and never log it. Its `Debug` is redacted and its owned
key buffer is zeroized on drop; copies held by the host or cryptographic working
state are not covered by that guarantee. `seal` returns authenticated bytes and
an `ApprovalPolicyCheckpoint`. The frame is `LSAP` plus version byte `0x01`, a
four-byte big-endian JSON byte length, UTF-8 closed-schema JSON, then a 32-byte
HMAC-SHA256 tag over the domain separator and complete frame preceding the tag.
Authentication uses [RustCrypto HMAC](https://docs.rs/hmac/0.12.1/hmac/), not a
custom MAC implementation. Restore authenticates the bounded (at most 2 MiB)
frame before parsing JSON, then rejects unknown fields, duplicate policy IDs,
invalid identity/budgets, and mismatched checkpoints.

The host must atomically publish snapshots together with separately protected
latest checkpoints, serialize all policy mutations under one owner, and provide
that authoritative checkpoint to `restore`. Never read the checkpoint from the
same untrusted file as the snapshot. Restore requires the exact store ID,
revision and snapshot SHA-256; replaying an old valid MAC against the latest
checkpoint fails. A MAC alone does not detect replay of both old files. Protect
the clock high-water mark too: matching/remembering and restore refuse backwards
time, while listing, revoking, clearing and sealing remain possible so an incorrect
clock cannot prevent removal of consent. Sealing retains the last valid high-water
mark if the clock is unavailable or earlier. Unpublished observations cannot
provide rollback protection across a crash/restart; a host must persist its time
checkpoint or use an independently protected clock before enabling reuse.

These snapshots are not encrypted: paths, routes and digests can be sensitive.
Protect snapshot confidentiality and do not upload them. The portable policy store
performs no disk IO; the opt-in Windows backend below supplies OS storage.
Independent unguarded restored instances do not synchronize
revocation. Restoring or matching a snapshot alone does not remove
`PLUGIN_WASI_APPROVAL_REQUIRED`. Crash-safe host persistence and application UI
remain pending; never treat an unprotected file as a remembered permission source.

### Opt-in remembered execution and live revocation

After binding the scope, a trusted host can attach one shared validated store using
`WasiComponentHost::with_remembered_policies(Arc<RememberedApprovalPolicies>)`.
`ActivatedRemoteWasiPlugin` exposes the same opt-in; profiles use
`prepare_with_remembered_policies(store, cancellation, trusted_domain, policies)`.
The profile backend still starts disabled. Ordinary hosts/preparation/CLI routes
remain default-deny and do not attach a store or infer human confirmation.

On a matching live policy the broker issues a fresh private one-action permit,
valid at most five minutes and never beyond the policy deadline. There is no
restored session quota, automatic policy-expiry extension, dialog or invocation
retry. Repeated in-scope calls renew their per-use permit without prompting;
issuing a new permit neither extends nor invalidates an earlier started use.
Covered pending descriptions for that host are dismissed, but are not an execution
queue. Other hosts/targets and oversized payloads remain pending. Explicit session
grants are checked first and retain their independent monotonic-clock semantics.

Every staged-write/publication and pre-network-dispatch checkpoint rechecks the
private permit, host attachment and shared policy authority. Store revocation,
clearing or policy replacement invalidate older permits across hosts sharing that
same store. A backwards/unavailable policy clock refuses remembered authority;
the controller keeps the actual pending request so a fresh independent explicit
confirmation remains possible. Nothing silently widens or renews the stored scope.

Host/configuration/controller changes and host destruction suspend that host's
attachment and started uses without deleting other hosts' remembered choices.
`SensitiveActionController::clear` also suspends all its current attachments: it
does not delete the shared durable policy records. The host must not silently
reattach after a user's session suspension. UI must distinguish stopping current
sessions from forgetting stored choices; forgetting requires store revocation and
publication of the new protected checkpoint. `remembered_uses` exposes currently
held permits separately from regular `grants`. Revoking a displayed active request
cancels its held uses and revokes their corresponding stored policies. After uses
finish, use the policy ID from the store for persistent revocation.

Authority is bounded to 128 attached host contexts and 128 active request identities
per controller, with at most 10000 held permits per identity. Completed permits are
pruned; capacity refusal is not a permanent blacklist. Restored independent stores
do not synchronize revocation: use one authoritative shared store per owner. Core
does not automatically select protected storage or synchronize independent processes,
and already-published files/native network effects cannot be undone.

### Live authoritative policy guard

`RememberedApprovalPolicies::with_authority_guard` is a trusted-host opt-in for
an external authoritative owner. `RememberedApprovalAuthorityGuard::check_current`
receives a v1 `RememberedPolicyIdentity` (store ID, policy ID and complete policy
SHA-256), not an execution token. Owners can derive the same identity with
`for_policy` from already authenticated policy metadata. It neither verifies an
untrusted snapshot nor proves consent. Never learn the authoritative record from
the first client request or consult a replayable adjacent checkpoint file.

The guard runs when issuing remembered authority and at each execution checkpoint.
The complete policy hash includes its scope/ceiling, target and approval/expiry
times, so an uncommitted replacement cannot inherit the owner's old approval.
Independent restored clients can be vetoed by one current owner even while their
old snapshot MACs still verify. Unrelated policy changes need not invalidate a
different policy's unchanged identity. A guard can only veto an already matched
local policy; it cannot create consent, broaden manifest limits or approve a new
scope. Ordinary single-owner stores remain unguarded unless explicitly configured.

Callbacks run without the Core controller or policy mutex, must never show UI,
and must honor their deadline. Each check is bounded to 50 ms or the earlier guest
execution/use deadline; late success is rejected. Cancellation and execution time
are rechecked after the callback, before publication or network dispatch. This
does not preempt a blocked/hostile native callback: trusted native code is not a
sandbox, so the provider must use bounded nonblocking/timeout-aware operations.
Failure creates no remembered authority; the actual pending request remains
available for an independent fresh user confirmation.

The portable interface does not supply IPC or authenticate an arbitrary owner.
A production host must select a protected provider, serialize publications and
provide fresh per-policy checks before enabling cross-process remembered reuse.

### Opt-in Windows protected owner

`WindowsProtectedApprovalCheckpoint::open(root, owner, persistence)` uses one exact
current-user generic Credential Manager entry for the dedicated key and latest
checkpoint. It never enumerates credentials or changes domain credentials,
certificates, registries or installation state. `LogonSession` is temporary;
`ThisUserThisComputer` persists for the same user's logons on this computer,
not machine-wide consent. The namespace binds the canonical root, opaque owner
and persistence mode. Moving the root does not silently migrate its identity.

The host must supply an existing trusted private root outside every guest grant.
The isolation hooks below reject physical grant overlap; this constructor alone
does not prove ancestor ACL/reparse isolation or protect against
native code running as the same user. Snapshot objects are authenticated but not
encrypted; private root access and confidentiality remain the host's responsibility.

For a new root, use `create_private_root(parent, name, owner, persistence)`:
the existing stable trusted parent must be selected by the host, and `name` must
be one child name. The OS applies a protected DACL at creation, before any
payload: effective current user, LocalSystem and builtin Administrators, each
full control with child inheritance. No existing target, parent ACL, privilege,
credential or installer configuration is modified. Invalid owner/name input
does not create a directory; a failure after creation leaves the new root for
inspection rather than deleting a possibly changed path.

For an already provisioned root, `open_verified_private_root` validates only;
it never repairs ACLs. These strict entries check the actual held directory's
owner and protected three-entry DACL, and recheck before credential reads,
publication locks, snapshot access and permission vetoes. Lock/staging/selected
files are also checked by handle for the same finite principal/access policy;
file inheritance flags may differ from directory flags. Null/unprotected root
DACLs, additional principals, other ACE forms and unexpected access masks fail
closed. Copies retained by restored policies keep this validation. This is a
deliberately finite policy, not a general Windows effective-access evaluator.
Low-level `open` retains its trusted-root precondition for compatibility.

An owned Windows fixture verifies root creation, rejection without repair,
existing-target refusal and inherited lock/snapshot permissions. A separate
exact-credential fixture retains remembered consent through a temporary root
or selected-file DACL violation: execution checks fail while unsafe, then the
same unexpired permit and repeated matched uses work after fixture repair,
without another remember-choice or a permanent blacklist. Only fresh fixture
children/files have their DACL changed, and exact owned test credentials and
temporary roots are removed on exit. Recovered permissions do not renew consent
expiry, expand its scope or undo a prior information disclosure. Deployment,
untrusted ancestor races, administrators/SYSTEM, native same-user malware and
logon/power-loss behavior remain separate acceptance boundaries.

`ApprovalStorageIsolation` is a trusted native permission veto, not consent.
Declare private storage on every host before any guest execution using
`WasiComponentHost::with_protected_approval_storage`; remote plugins expose the
same method, and profiles use `prepare_with_protected_approval_storage`. This
route does not enable a profile backend, attach remembered policies or issue
grants. Restoring through the Windows backend puts its isolation on the policy
store automatically; `with_remembered_policies` propagates it before attachment.
Other hosts, including those with remembered reuse disabled, still need an
explicit declaration: Core does not keep an implicit process-global directory list.

The Windows provider compares actual directory handles, normalized volume-GUID
paths and ancestor file identities. Same roots, grant ancestors/descendants and
case/label aliases refuse attachment; path text cannot substitute for a held
directory capability. Siblings are allowed. It supports at most 128 filesystem
grants, 128 ancestor entries per chain and 32768 UTF-16 path units; unresolved
volume paths or inaccessible identities fail closed. This does not certify root
DACL confidentiality by itself, hostile ancestor races, network filesystems or
native same-user attackers. The app must still provision a stable private
directory, preferably through the strict entries above.

Hosts retain up to 16 vetoes, recheck them before guest execution and filesystem
broker IO, and recheck before staged publication even for explicit session grants.
Callbacks run without controller/policy locks, never open UI or call guests, and
have a 50 ms budget clipped to execution time; late success is rejected. Registered
private-storage reads traverse parent directories without following links, which
also rejects junction traversal into private storage. Ordinary unregistered read
routes keep their existing behavior. Controller/session suspension does not remove
storage exclusion. Correcting a temporary veto permits fresh explicit confirmation;
it is not a blacklist or permission-expansion mechanism.

`initialize` and snapshot-identity-CAS `publish` take a shared OS file lock, validate
authenticated metadata, sync a bounded immutable snapshot, publish its new name
without overwriting an existing object, then update the protected checkpoint.
CAS compares store ID, revision and snapshot digest. Concurrent clock-only advances
do not cause false policy-update conflicts, and publication never lowers the
current protected clock floor. Same-revision publications cannot change policies.
Publication validation deliberately does not require a working wall clock, so
revocation/forgetting can be saved during clock rollback; execution still rejects
that clock. All writers must use this backend and the same trusted root.

`restore_policies(deadline)` reads only the object selected by protected storage.
Missing/corrupt selected data never falls back to an older snapshot. A write cut
before checkpoint selection leaves an unselected object, not new consent. Restore
returns a store with a protected clock source and live owner guard, not a host
attachment or permission to run. The returned store persists every successful
clock observation before using its time. The backend also implements
`RememberedApprovalAuthorityGuard`, which vetoes stale clients using the current
authenticated policy identity and rejects expired policies. Native low-level
`load` plus manual portable `restore` does not install these safeguards.

`observe_time(deadline)` monotonically updates only the protected checkpoint's
time, not its snapshot digest/revision or policy expiry. Existing authenticated
snapshot time may be below the separately protected floor; it may never be above
it. Repeated observations in the same second require no write, and observations
create no snapshot files. A failed/late update grants no use. Backwards or
unavailable time still permits authenticated revocation publication, retaining
the latest floor. Once the clock is corrected, fresh confirmation remains usable.
The credential schema and snapshot framing are unchanged.
Do not enable reuse from restore alone or silently retry a failed publication.
Native writes finishing after a deadline may already have committed; reread the
protected checkpoint before deciding what happened.

Current evidence covers real Credential Manager read/write, file-backed restore,
unselected-object recovery, corruption refusal, forgetting, independent-client
guard revocation and competing backend instances on Windows threads. A separate
owned fixture launches two real OS child processes, barriers their independent
authenticated views, then verifies one CAS winner, one stale-view conflict and
cross-process policy veto. Its parent bounds waits, terminates only its own children
on fixture failure, and removes only the exact owned credential/temporary root.
The ignored worker entry is transport scaffolding, not an independent verification.
Another owned fixture terminates real child processes at six publication cuts:
staging creation, first 64 KiB write of a larger snapshot, file sync, object link,
staging-name removal, and protected-checkpoint publication. Before selection,
restart recovers the old selected policies; after checkpoint publication it recovers
the new ones without replaying a stale update. Test probes exist only in test builds.
This does not yet establish power-loss durability, logoff
persistence, Linux/macOS providers, application UI or release
security approval. File sync is not a promise of directory-entry power-loss
durability. Native key-store calls are trusted blocking OS calls, not preemptible
sandbox operations; deadline overruns fail after return. The native credential
tests are ignored by default and run explicitly with test-owned credential cleanup.
Protected-clock tests cover fresh independent restoration after rollback, expired
policy non-revival, unchanged snapshot count during repeated reuse, and forgetting
with a broken clock. Policy clocks use a 50 ms native operation budget; unlike
authority callbacks, a clock can run under its own Core policy mutex. The backend
never calls that store while holding the owner lock, so lock order cannot recurse;
no clock read opens UI or calls guest code. This is not a wall-clock rollback
benchmark or proof of OS persistence across power loss/logoff.

`collect_unselected_snapshots(max_objects, deadline)` is explicit host maintenance,
not a general file-delete API or a guest capability. Under the same owner lock it
first authenticates the selected object; if selected data is missing/corrupt,
collection stops before deleting anything. It never deletes the selected snapshot
or changes the protected checkpoint/consent. A snapshot must match this owner's
filename digest and authenticated store ID. New staging names bind owner and
random nonce with a separate HMAC domain; single-linked partial staging objects
can therefore be recognized after termination. A two-link crash pair is removed
only after authenticating the complete snapshot and matching both file identities.
Unexpected links, malformed objects, other owners, directories, wrong-store data
and old unauthenticated staging names are retained. No recursive traversal occurs.

Limits are 1..128 removed file entries, at most 4096 scanned directory entries,
2 MiB per object and an aggregate 8 MiB read budget. Object-budget exhaustion is
reported in `ApprovalSnapshotCleanup`; scan/read exhaustion, IO failures or deadline
expiry return an error, possibly after some unselected objects were removed.
These discarded cache objects are not recoverable through this API; current consent
and unverified data are preserved. Maintenance must run outside latency-sensitive
guest work; a blocked clock/guard check keeps the independent confirmation route.
Native tests cover bounded collection, selected-object corruption refusal, wrong
keys/store IDs, personal-file and hardlink preservation, and idempotent collection
after actual child termination. Native same-user attackers and hostile root
ancestors remain outside this trusted-private-root backend's guarantee.

For UX, distinguish approval once, a bounded session/batch, and a separate explicit
remember-choice. Repeated matching actions reuse remembered consent without new
dialogs; a five-minute per-use permit is an internal lease, not a five-minute
reapproval requirement. Revocation, expiry or changed binding/scope requires fresh
consent. A remembered checkpoint/clock failure alone disables remembered reuse,
not independent fresh confirmation. A separate private-storage isolation veto
still applies to that guest host, even after fresh confirmation: unsafe storage
must be repaired or the host explicitly reconfigured safely, not bypassed by
another consent dialog. Recovery is rechecked; never turn a transient failure
into a permanent blacklist.

### Core grant lifecycle and enforcement

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
applied through the owned staging file handle rather than reconstructing Windows
attributes from a directory permission adapter.
Windows existing-file staging also reads owner/group/DACL, including inheritance
protection, from the original file, creates the empty staging file with that
security descriptor and verifies the resulting identity before writing payload.
It does not first write sensitive bytes under broader default directory permissions.
Before publication it rechecks the original security identity; ACL-only changes
also cause `PLUGIN_WASI_WRITE_CONFLICT`. Descriptor/string/path buffers are bounded,
and only a security digest is retained, not logged raw SID/SDDL. If preservation
requires privileges the process lacks, the write fails without altering the target;
no privilege enabling or ignore-ACL fallback is performed. The staging filename is
one safe component, and native `CREATE_NEW` never overwrites a collision.
These checks were exercised on controlled Windows x64 files, not every filesystem
or enterprise policy. Windows basic-metadata capture now preserves positive
creation time and supported hidden/system/temporary/not-content-indexed flags.
New content is marked ARCHIVE for backup; it does not restore old last-write,
last-access or metadata-change times. Absent targets retain native defaults.
Before data-read opens, preflight inspects the no-follow metadata's finite flags;
readonly files are refused without changing their attributes or opening approval.
Unsupported offline/recall/pinning/no-scrub/unknown attributes are rejected rather
than silently erased. This is not a guarantee against native path races or cloud
minifilter hydration behavior, and these providers have only flag-classification
coverage, not live cloud-driver acceptance.

Creation/last-write times and supported flags are rechecked on the original;
time-only, attribute-only and readonly concurrent edits conflict without publishing.
Staging is checked before payload and again before rename, after permission changes;
only its held handle receives basic metadata and permission writes. Owned NTFS
fixtures verify actual replacement, backup marking, natural new-content write time,
readonly preservation and fresh operation recovery after conflicts. Compression
and named-stream preservation remain separately checked. The finite
[basic-information API](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)
does not establish all timestamps, drivers or extended metadata as supported.
Unix owner identity, Windows SACL/security resource attributes,
encryption, sparse/integrity semantics and other extended metadata remain open.
Existing files with encrypted, sparse, reparse or integrity flags are refused in
storage preflight, before creating an approval request, with
`PLUGIN_WASI_METADATA_UNSUPPORTED`, unless the no-follow path policy already refused
the target. If detected during an approved write, the
candidate is discarded without publication. Unsupported compression formats also
fail explicitly; consent does not bypass unsupported preservation. This does not
make those file types supported. Controlled real sparse fixtures exercise preflight;
EFS is covered only by flag classification, without creating certificates or
enabling privileges.
NTFS compression NONE/LZNT1 is captured separately for the original primary stream
and each named stream. Owned staging handles apply the observed format before
payload writes; primary and named formats are rechecked before publication,
including compression-only changes. New targets retain native directory defaults.
Fixed-size synchronous control calls use Microsoft's
[get](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_get_compression)
and [set](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_set_compression)
contracts, not a generic cross-filesystem storage clone.
Windows named `$DATA` streams are now enumerated with a fixed, eight-byte-aligned
64 KiB buffer. The parser bounds linked offsets, UTF-16 names (1024 bytes), types,
duplicates and 128 named records. Named data is bounded in aggregate by the lesser
of the input budget and 4 MiB; empty, Unicode and binary streams are retained,
including `Zone.Identifier`. Unknown types, invalid names/UTF-16, unsupported
enumeration and excess budgets fail without silently dropping metadata. Driver and
filesystem support is separate; see Microsoft's [stream information contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_stream_info).
Named data is read into a bounded private snapshot, copied only to the owned staged
file in checkpointed 64 KiB chunks and synced. Native stream opens derive the path
from the pinned directory handle, validate base/suffix separately, and do not follow
the final reparse point; single-letter base names are not parsed as drive prefixes.
Creation requires an existing regular staging base and denies delete sharing while
opening/writing the stream, so it cannot implicitly create a missing base with a
broader DACL or overwrite an existing stream. Before publication both original and
staged stream sets/content/compression are rechecked. A stream-only edit is a write
conflict;
cancellation, expiry and revocation stop at the same existing consent checkpoints,
without another approval use or dialog. Deleting an owned staged base removes its
streams too. Stream bytes/names can be sensitive and are never logged or uploaded.
This retains observed streams, not a global atomic metadata/filesystem CAS or a
complete security/extended-attribute clone. Controlled NTFS x64 evidence does not
prove every filesystem, remote share or enterprise policy.
Grants remain package-relative; do not use this implementation as a
general-purpose editor for external security-sensitive files. Platform metadata
preservation, multi-file transactions and crash recovery remain separate work.
`ReplaceFileW` is not used as a shortcut: Microsoft's documented 1176/1177 failure
states can remove or rename the original without completing replacement. Its merge
benefits do not by themselves establish the required failure-safe transaction.
See [Microsoft's replacement contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew).
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
