# PP-FormulaNet KV-cache contract

## Baseline status

The current production PP-FormulaNet-S package does not expose an ONNX
incremental decoder. Commit `85988d1` removed the reconstructed ONNX fallback
because it had semantic drift; `models/formula-rec/pp-formulanet-s/config.json`
records that decision. Production uses the official Paddle full inference
program, where the while loop and KV cache remain internal to Paddle.

Consequently, this repository cannot truthfully freeze the requested 29-entry
Paddle-variable/ONNX-input/ONNX-output mapping or prove the `Add.34` root cause
from the current model assets. `decoder_step.onnx`, its exact exporter commit,
and step-0/step-1 reference tensors are required inputs. An empty or inferred
mapping is not accepted as evidence.

## Fail-closed schema

`latexsnipper_inference::DecoderStateSchema` is the versioned contract for any
future incremental decoder export. Every entry binds a semantic name, Paddle
variable, ONNX input, ONNX output, dtype, rank, axis semantics, observed
step-0/step-1 shapes, growth axis, layer, attention kind, update rule, and
encoder-static flag.

Validation enforces:

- unique semantic/input/output mappings;
- rank and axis agreement;
- self-attention K/V growth on a declared sequence axis;
- static cross-attention K/V with no growth axis;
- stable batch and beam axes;
- monotonic self-cache sequence growth;
- stable encoder sequence length;
- exact dtype and observation mapping.

Errors carry the stable prefixes `CACHE_SCHEMA_MISMATCH`,
`CACHE_SEQUENCE_NOT_MONOTONIC`, `CACHE_BATCH_MISMATCH`,
`CACHE_DTYPE_MISMATCH`, and `CACHE_OUTPUT_MAPPING_MISMATCH`.

## Evidence workflow

Once the exact removed graph is supplied:

1. Run `tools/decoder/trace_onnx_node.py --model decoder_step.onnx
   --node Add.34 --output trace.json`.
2. Capture every loop-carried tensor at step 0 and step 1 as NPZ and run
   `tools/decoder/dump_step_state.py`.
3. Populate `tests/fixtures/decoder/state-schema.json` from graph names and
   captured shapes. Do not assign meanings by position.
4. Capture full and incremental tensors at T=1/2/3/6/9/15/30 and run
   `tools/decoder/compare_incremental.py`.
5. Stop at the first divergent step and retain the complete state dump.

The graph tracer follows the shape-sensitive
Reshape/Transpose/Slice/Concat/Expand/Gather/Unsqueeze ancestry and emits JSON.
The comparator emits max/mean absolute error, cosine similarity, and top-1/
top-5 agreement. No repeated-token penalty participates in this workflow.
