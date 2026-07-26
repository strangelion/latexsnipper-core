# Recognition provenance schema v1

Recognition provenance and post-processing evidence are additive fields on the
v3 AST. They do not replace the formula source used for rendering.

## Compatibility contract

- `recognition_provenance` and `recognition_evidence` may be absent or `null`.
- Readers must continue to accept documents written before either field existed.
- Unknown fields inside `RecognitionProvenance` and
  `TransformationEvidence` are ignored so evidence producers can add detail.
- The formula's `source` remains authoritative. Raw, normalized, and corrected
  model output is evidence, not an implicit source rewrite.
- Transformation hashes are SHA-256 over the exact before/after UTF-8 strings.
- API/FFI consumers must treat new enum variants and evidence fields as
  forward-compatible additions.

The compatibility test
`provenance_json_is_backward_and_forward_tolerant` covers a legacy snapshot,
missing evidence, explicit `null`, and unknown future evidence fields.

## Size boundary

The Windows x86_64 measurement records `Inline` at 712 bytes in both the
baseline and current tree. `Formula` grows from 672 to 688 bytes (16 bytes,
2.38%), below the 25% review threshold. Registry-based evidence storage is only
an evaluation prototype; no AST representation change is made by this work.

