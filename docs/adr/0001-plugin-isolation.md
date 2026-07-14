# ADR 0001: Plugin isolation classes

Status: accepted

Trusted built-in Rust plugins use bounded in-process workers, cooperative
cancellation, soft deadlines, and quarantine. The host must never claim it can
force-kill arbitrary Rust code. Reviewed local external plugins use versioned JSON
IPC in a dedicated child process with kill-and-wait cleanup, memory limits, a
response-file observation limit, an empty environment, and platform process
limits. Unix creates a dedicated session/process group and kills the group on
timeout. Windows assigns the child to a kill-on-close Job Object after spawn;
the small pre-assignment race remains a documented limitation.

Native Rust trait-object ABI is rejected because compiler layout is not stable.
The process IPC protocol is the current stable external interface. A WASI Component
host remains the preferred future boundary for untrusted third-party code. Process
plugin permission grants cover brokered host operations only: arbitrary native
filesystem and network calls are not OS-sandboxed. Descendants that deliberately
escape the Unix session, the Windows pre-assignment race, and total workspace disk
usage also remain outside the current containment contract.
