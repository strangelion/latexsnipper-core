# Native C registered formula conversion

The additive `latexsnipper_formula_registry_*` symbols use the existing v3 JSON
envelope and `latexsnipper_string_free`. They are separate from OCR sessions and
the old model-free formula functions. ABI declaration:
[latexsnipper_session.h](../../crates/ffi/include/latexsnipper_session.h).

## Lifecycle and routing

Create with `{"schemaVersion":1}` for built-ins. A build with the explicit
`wasi-formula` feature also accepts `profile` (a closed v1 profile object) and
`pluginStore` (an existing store path), together. Preparation only activates an
already verified and enabled package. It never installs, downloads or enables a
package. Registration leaves the profile backend disabled; enable the route with
`set_enabled`, then select it in the request or an explicit availability policy.
The shared [profile and registry contract](conversion-registry.md) still applies.

```json
{
  "schemaVersion": 1,
  "input": "latex",
  "output": "omml",
  "mode": "strict",
  "backend": null,
  "content": "\\frac{1}{2}",
  "contextSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "policy": { "schemaVersion": 1, "backends": [] }
}
```

The sample digest is a placeholder, not a production cache identity. Supply an
actual fingerprint of the Core binary/build, definitions, fonts, styles and host
dependencies. A valid outer `ok:true` means the report was returned, not that
conversion succeeded: inspect `data.outcome.kind` (`converted` or `rejected`).
Only a converted result contains a candidate. Reports bind input, route, backend,
configuration and decisions, not document write authority. Policy/request JSON
errors use outer failures; valid registry refusals retain their audited report.
The default backend is only `core:builtin`; availability-only fallback is explicit,
and an executed handler's failure, trust revocation or cancellation is terminal.

Handles retain the registry and compiled WASI host across calls. At most 64 live
or preparing instances are allowed. Preparation reserves capacity without holding
the global handle lock while compiling. Calls to one busy registry fail with
`FORMULA_REGISTRY_BUSY`, without an implicit queue. Cancel and close do not acquire
its conversion mutex. Cancellation is terminal for that instance, not a permanent
plugin refusal: explicitly create a new instance for subsequent work. Close
removes the handle, signals guest interruption and revokes its grants. Work already
inside native IO cannot be undone; candidates observed after cancellation/closure
are suppressed. The last check and response are not an atomic cancellation barrier.

Creation JSON is capped at 32 KiB, profiles at 16 KiB, other request JSON at 1 MiB,
approval commands at 16 KiB and serialized response `data` at 2 MiB. The registry
retains its 64 KiB source and 256 KiB candidate limits; the larger response budget
allows JSON escaping and report metadata, not larger formula candidates. These are
observed transfer bounds, not process peak-memory or native hard-deadline limits.
The fixed host worker stack does not change guest memory/fuel/deadline grants.

## Trusted host approval

`approvals` returns pending requests and live grant snapshots, never approves them.
The snapshots can include sensitive file paths and network destinations; do not
log or upload them automatically. Only a trusted native application host may call
`approval` after explicit human consent. Do not expose this function directly to
guest code, MCP callers, UI scripts from plugins or remote messages.

Commands are closed JSON: `once` with `requestId`; `approve` with `requestId`,
`scope`, `uses`, `validForMs`; `batch` with `requestIds`, `validForMs`; `revoke`
with `requestId`; or `clear`. Scope is `{"kind":"exact"}` or
`{"kind":"sameTarget","maxPayloadBytes":N}`. Core validates the existing owned
request, identity, finite quotas and TTL (at most one hour), with no wildcard or
permanent grant. Valid grants do not re-prompt for every chunk/action. Revoke and
query remain callable while conversion is busy. Package/route enablement does not
approve side effects. Missing approval stops that action and does not auto-retry
the guest invocation. This C ABI does not implement an application dialog or
persist human policy across process/session restarts.

## Verification and remaining bindings

The finite local power fixture exercises real importer/exporter output through C
entry points, disabled routes, package disablement after preparation and terminal
failure without fallback. It is not general AsciiMath/UnicodeMath/MTEF support or a
signed remote service test. `formula_registry_smoke.py` also loads an actual native
shared library to verify exported symbols, ownership, reports and lifecycle.
CI runs it alongside legacy formula/session/JNI checks. The script is a ctypes
reference client; the formal PyO3 and WASM/JS registered-conversion APIs, application
UI, binary/document routes and real Office writeback remain separate work.
