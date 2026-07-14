# ADR 0001: Plugin isolation classes

Status: accepted

Trusted built-in Rust plugins use bounded in-process workers, cooperative
cancellation, soft deadlines, and quarantine. The host must never claim it can
force-kill arbitrary Rust code. Reviewed local external plugins use versioned JSON
IPC in a dedicated child process with kill-and-wait cleanup, memory and output
budgets, an empty environment, and platform process limits.

Native Rust trait-object ABI is rejected because compiler layout is not stable.
The process IPC protocol is the current stable external interface. A WASI Component
host remains the preferred future boundary for untrusted third-party code because
the process host does not yet provide complete filesystem/network sandboxing.
