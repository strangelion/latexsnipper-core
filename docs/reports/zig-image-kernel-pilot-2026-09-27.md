# Zig image-kernel pilot — 2026-09-27

## Decision

Do not add Zig to the Core build at this stage. Keep the current Rust image
kernels and revisit only after profiling identifies a different native hotspot.

## Method

Zig 0.15.2 was compiled as a static C-ABI library on Windows x86_64. Rust owned
all input and output buffers and retained a pure Rust correctness oracle. Both
pilots were built in optimized release mode and measured in the same process.

The exploratory runs used 200 iterations of 640 x 640 RGB normalization and 100
iterations of a 1920 x 1080 RGB perspective-warp input producing a 1024 x 512
output. The measurements are local engineering evidence, not cross-platform
release claims.

| Kernel | Rust total | Zig total | Rust / Zig | Result |
|---|---:|---:|---:|---|
| HWC u8 to CHW f32 normalization, initial loop | 827.069 ms | 905.326 ms | 0.914x | rejected |
| Normalization, channel-major Zig loop | 578.731 ms | 576.073 ms | 1.005x | below 1.20x gate |
| Bilinear perspective warp | 163.403 ms | 206.077 ms | 0.793x | rejected |

All five supported pixel layouts matched the Rust reference implementation in
the normalization pilot within `1e-6`. The perspective-warp pilot matched
exactly for Gray, RGB, RGBA, BGR, and BGRA fixtures. Correctness was sufficient;
performance was not.

## Follow-up

Before another Zig experiment, collect profiles on Windows, Linux, macOS, and
Android ARM64 with representative OCR workloads. A future pilot must remain
behind an opt-in feature, keep allocation ownership in Rust, and demonstrate at
least 20% median latency improvement without material memory growth. Formula
layout, model/runtime selection, parsers, and security policy remain Rust code.
