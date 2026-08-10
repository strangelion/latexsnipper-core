# tract-nnef 0.21.16 security backport

This vendored crate retains the `tract-nnef` 0.21.10 implementation required by
the production formula-recognition model and backports the dense tensor parser
overflow checks from upstream commit `34c7df2c9bd2a36583e09b52f3e6319bf23102e8`.

The package version is `0.21.16` because it contains the complete fix for
RUSTSEC-2026-0217. Other 0.21.16 behavior changes are intentionally excluded:
upgrading the full tract graph translator changes ONNX `Range` typing and makes
the frozen production decoder model fail during graph construction.
