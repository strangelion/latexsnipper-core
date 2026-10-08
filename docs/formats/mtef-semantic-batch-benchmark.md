# Experimental MTEF semantic batch reuse benchmark

Measured 2026-10-09, Windows x86_64, Intel Core i5-13500H (12 cores/16 logical
processors), rustc 1.96.0, release build. This is a single-threaded in-memory
comparison of the experimental finite v5 reader, **not Office or MathType
conversion speed, real-producer compatibility, visual fidelity or accuracy**.
The fixtures and numeric/header streams are repository-authored.

Each case has 10,000 occurrences, one warmup and five measured trials. Inputs
are prepared before timing. Direct reading retains all individual reports;
batch reading retains shared reports and per-occurrence input references.
Both include framing, AST construction, canonical LaTeX and losses. Validation
and result destruction are outside timing. Direct reading runs first in each
trial; results are a local snapshot, not a universal latency guarantee.

| Authored case | Unique byte streams | Direct median (ms) | Batch median (ms) | Direct / batch |
| --- | ---: | ---: | ---: | ---: |
| Same fraction repeated | 1 | 23.7350 | 0.2621 | 90.56x |
| Five variable/fraction/root/script/matrix streams | 5 | 23.9035 | 0.2601 | 91.90x |
| 1,250 numeric streams, each repeated eight times | 1,250 | 14.2397 | 2.2130 | 6.43x |
| Same variable, 10,000 distinct header application keys | 10,000 | 9.9038 | 11.7156 | 0.85x |

The no-reuse control is about **18.3% slower** due to indexing/budget bookkeeping;
batching is not universally faster. Its equal canonical outputs still cannot
be shared because source metadata differs. Hosts should assess their actual
repetition and IO/COM costs, not extrapolate these ratios to document conversion.

All compared LaTeX and loss reports agreed, and each occurrence retained its own
input pointer. This is consistency against the same reader, **not an independent
accuracy score**. [Raw measurements](../generated/mtef-semantic-batch-benchmark.json)
include sorted trial times, work/storage counters and UTF-8/LF source digests.

## Reproduce

```sh
cargo run --locked --offline --release -p latexsnipper-conversion --no-default-features --example mtef_semantic_batch_benchmark
```

The operation is opt-in Rust-only, scoped to one call, using full raw-byte
equality. It does not cache by shape or canonical output, persist across calls,
activate an OLE server, write documents or register production MTEF support.
Limits and unsupported cases remain in the [finite semantic profile](mtef-semantic-v1.md).
