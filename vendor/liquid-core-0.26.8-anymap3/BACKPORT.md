# Liquid register backend maintenance backport

Base: the published `liquid-core` 0.26.8 crate, SHA-256
`feb8fed70857010ed9016ed2ce5a7f34e7cc51d5d7255c9c9dc2e3243e490b42`.
Upstream licenses and attribution are retained. `Cargo.toml.orig` records the
original upstream manifest and is not the build manifest.

The maintenance change updates the effective manifest and the two private `Registers`
references in `src/runtime/runtime.rs`: replace `anymap2` 0.13 with the existing
maintained `anymap3` 1.1.0 dependency. This removes the unmaintained package rather
than renaming it or ignoring RUSTSEC-2026-0319. The public register API, `RefCell`
borrow rules and template implementation remain unchanged.
Two compiler-hygiene edits remove redundant trait-object parentheses in
`src/model/ser.rs` and spell the elided return lifetime in `src/parser/registry.rs`.
They preserve the original types and avoid warnings on current Rust toolchains.
One mixed-tab indentation line in `src/parser/grammar.pest` is normalized to spaces;
the grammar expression is unchanged.

Tract 0.21.10 pins Liquid 0.26.8 in its kernel generator. Keeping that version
avoids an unrelated graph-stack upgrade that would break the frozen production
decoder model. `crates/tract/tests/liquid_registers.rs` exercises default values,
mutations, independent types/instances and recovery after a conflicting borrow.
The normal Tract build also regenerates its native kernels with this backport.
Real-model/profile compatibility remains subject to the production-profile gate;
register tests alone do not prove it.
