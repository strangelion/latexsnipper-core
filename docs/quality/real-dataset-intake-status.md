# Real dataset intake status

The checked-in formula v1 corpus contains 50 deterministic synthetic fixtures.
It is suitable for repeatable contract and failure-analysis runs, but is not a
substitute for real-distribution quality evidence.

The following required strata remain blocked because no redistributable,
redaction-reviewed inputs with per-item source and license were available:

| Stratum | Required minimum | Current admitted real samples |
|---|---:|---:|
| Screenshots | 30 | 0 |
| Scans | 20 | 0 |
| Mobile photos | 20 | 0 |
| Hard negatives | 20 | 0 |
| Mixed table images | 30 | 0 |

Admission requires an immutable image SHA-256, source, license, capture class,
ground truth, and redaction review for every item. Formula images additionally
record scale/degradation and expected kind (`formula` or `hard_negative`).
Table images require ordered rows/cells, spans, content kind, content, and
coordinates. Predictions must come directly from a named model bundle and may
not be copied from ground truth.

No local personal files, screenshots, downloads, or duplicated fixtures were
admitted to satisfy sample counts.

