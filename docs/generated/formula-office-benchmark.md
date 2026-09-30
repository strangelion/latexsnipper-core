# Formula and Office corpus benchmark

> Generated evidence. Contract pass rates are not model accuracy on independent real-world data.

## Evidence identity

| Field | Value |
|---|---|
| Plan | `formula-office-10k-v2` |
| Tier | `Full` |
| Corpus SHA-256 | `92c316ab0d26f0425d0ae55ef89412a9813e6027b3f21cfd5ce457b99722c378` |
| Source commit | `94c762e57daa8a8e1500c7d480e7df8503b6eff1` |
| Generated at (UTC) | `2026-09-30T03:13:09Z` |
| Environment | Local Windows x86_64 release validation; rustc 1.96.0 (ac68faa20 2026-05-25); median-throughput run of 3 identical processes |
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
| Parse | 5.000 us | 14.900 us | 30.600 us |
| Conversion | 23.000 us | 109.700 us | 212.500 us |
| Round trip | 22.200 us | 529.700 us | 978.900 us |

- Total measured time: **6.249 s**
- Throughput: **1600.37 records/s**
- Peak process resident memory (lifetime high-water mark): **66875392 bytes (63.78 MiB)**

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

- peak resident memory is the process lifetime high-water mark and includes startup, corpus loading, evaluation, and report construction
- the deterministic full tier is synthetic contract-scale evidence, not accuracy on 10,000 independent real-world formulas
- Microsoft Office application fidelity requires the external Office harness

## Reproduce

See [`docs/benchmark.md`](../benchmark.md) for the digest-frozen generation and benchmark commands.
