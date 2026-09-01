# Runtime admission scheduling

`latexsnipper_runtime::RuntimeAdmissionScheduler` is the shared, bounded
admission policy for desktop, service, and mobile adapters. It deliberately
does not own threads or execute model code: callers submit a job, start work
only after an admission or queue-promotion result, and release the job on every
success, cancellation, timeout, or error path.

The policy controls four finite resources:

- active CPU jobs;
- active accelerator jobs;
- queued jobs;
- estimated active model memory.

Jobs declare `interactive`, `foreground`, or `background` priority. Queue order
is priority-first and FIFO within one priority. An accelerator request uses CPU
only when `allow_cpu_fallback` is explicitly true; the scheduler never invents
a fallback. A job larger than the complete memory budget, an invalid ID,
duplicate ID, unavailable resource, or full queue fails closed with a stable
`RuntimeAdmissionCode`.

Completing or cancelling active work releases its slot and promotes every
highest-priority runnable job without allowing an accelerator-only head item to
block unrelated CPU work. `snapshot()` exposes deterministic active and queued
state for diagnostics without exposing runtime sessions or model bytes.

The scheduler is policy, not a background executor. Adapters remain responsible
for associating the returned job ID with their task, invoking Core recognition,
and calling `complete` or `cancel` exactly once.
