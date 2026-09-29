# Formula and Office corpus benchmark

> Generated evidence. Contract pass rates are not model accuracy on independent real-world data.

## Evidence identity

| Field | Value |
|---|---|
| Plan | `formula-office-10k-v2` |
| Tier | `Full` |
| Corpus SHA-256 | `92c316ab0d26f0425d0ae55ef89412a9813e6027b3f21cfd5ce457b99722c378` |
| Source commit | `c56d63587faad0e04d75003dabdda27e4f964df3` |
| Generated at (UTC) | `2026-09-29T00:02:45Z` |
| Environment | Local Windows x86_64 release validation; rustc 1.96.0 (ac68faa20 2026-05-25) |
| Seed | `20260928` |
| Formula records | `10000` |
| Planned compound documents | `500` |

## Contract results

| Measurement | Result | Status |
|---|---:|---|
| Expected outcomes | 10000/10000 (100.00%) | **Verified** |
| Parse success | 100.00% | **Verified** |
| Attempted semantic conversions | 100.00% | **Verified** |
| Attempted semantic round trips | 100.00% | **Verified** |
| Visual/Office targets | 48000 deferred observations | **Not measured here** |

## Performance

| Stage | P50 | P95 | P99 |
|---|---:|---:|---:|
| Parse | 2.000 us | 3.400 us | 5.000 us |
| Conversion | 10.700 us | 38.300 us | 50.600 us |
| Round trip | 10.300 us | 146.000 us | 184.900 us |

- Total measured time: **1.986 s**
- Throughput: **5035.26 records/s**
- Peak process memory: **Not measured**

## Capability boundary

| Capability | Status | Evidence boundary |
|---|---|---|
| Formula parsing and declared error outcomes | **Verified** | Deterministic compositional corpus |
| LaTeX, MathML, OMML and Typst semantic conversion | **Verified** | Attempted Core conversions only |
| Semantic conversion back to LaTeX | **Verified** | Attempted Core round trips only |
| SVG, PNG and Office package visual fidelity | **Deferred** | Requires visual/package evidence layers |
| Word, Excel and PowerPoint application behavior | **Not measured** | Requires installed-Office automation |
| OLE activation, clipboard paste and field recalculation | **Not measured** | Requires the external Office harness |
| Real-world formula/model accuracy | **Not claimed** | Requires a licensed representative corpus |

## Known limitations

- peak process memory is not measured by the portable corpus runner
- the deterministic full tier is synthetic contract-scale evidence, not accuracy on 10,000 independent real-world formulas
- Microsoft Office application fidelity requires the external Office harness

## Reproduce

See [`docs/benchmark.md`](../benchmark.md) for the digest-frozen generation and benchmark commands.
