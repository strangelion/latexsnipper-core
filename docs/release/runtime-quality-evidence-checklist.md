# Runtime quality evidence checklist

Use this checklist before claiming the remaining runtime quality gaps closed.

- [ ] Evidence JSON reports a clean source commit and all command exit codes 0.
- [ ] Workspace format, check, test, documentation test, and clippy gates pass.
- [x] Fixed CPU model smoke creates a session and produces a hashed output.
- [x] DirectML smoke matches CPU within the recorded tolerance on Windows.
- [ ] CUDA, TensorRT, and CoreML smoke run on matching physical CI runners.
- [x] mmap experiment records first/warm inference, process memory, update,
  deletion/replacement, and cleanup without changing the production default.
- [x] AST legacy/missing/null/future-field compatibility tests pass.
- [x] AST size growth stays below 25%, or an approved design review is linked.
- [x] Formula predictions and metrics name the exact model/runtime/provider and
  preserve raw, normalized, and corrected output.
- [ ] Licensed real screenshot/scan/mobile/hard-negative thresholds are met.
- [ ] Licensed 30-image table corpus and real TEDS/cell metrics are present.
- [ ] A runnable incremental decoder artifact has runtime state shapes,
  semantic mapping, and Add.34 reproduction evidence.
- [x] Debug crop persistence is explicit-consent only, bounded, and off by
  default.
- [x] Dependency duplication is classified; security-sensitive convergence is
  deferred to coordinated upgrades.
- [ ] Privacy scan finds no username, home path, temporary path, secret, or
  unrelated external filename in tracked evidence.

Unchecked items are release blockers for the corresponding capability claim;
they do not prevent shipping unrelated, already validated functionality.

