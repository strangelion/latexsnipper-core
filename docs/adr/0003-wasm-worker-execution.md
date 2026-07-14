# ADR 0003: Web Worker execution and cancellation

Status: accepted

Active Tract inference cannot be interrupted by cooperative Rust cancellation.
The official TypeScript client therefore serializes recognition in one Web Worker.
Cancelling an active request terminates the worker, suppresses stale events, creates
a new worker on demand, and reloads checksum-verified model bytes. Queued requests
can be cancelled without restart.

Direct main-thread WASM calls remain available for compatibility and warn once in a
browser. Their cancellation mode is `cooperative-stage-boundary` and capability
metadata explicitly reports `canInterruptActiveInference: false`.
