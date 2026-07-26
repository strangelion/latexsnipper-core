# Dependency duplication audit

Command: `cargo tree -d --edges normal,build`.

This audit separates actionable direct dependencies from transitive version
families owned by independent upstream stacks. No broad upgrades were made
solely to reduce the duplicate count.

| Family | Versions | Owners | Risk and decision |
|---|---|---|---|
| `base64` | 0.13, 0.22 | `spm_precompiled`; core/`ureq`/`usvg` | Low runtime risk, but 0.13 is fixed by tokenizer internals. Keep. |
| `digest`, `sha2`, `block-buffer`, `crypto-common`, `cpufeatures` | 0.10 and 0.11 families | project SHA-256; `lopdf` crypto | Security-sensitive API generation boundary. Keep until `lopdf` and project consumers can move together. |
| `getrandom`, `rand`, `rand_core`, `rand_chacha` | 0.2/0.3/0.4 and 0.8/0.9/0.10 | image processing, tokenizer, PDF | Semver-incompatible transitive generations. Keep; do not patch RNG stacks. |
| `ndarray` | 0.16, 0.17 | project tensor adapters; ORT rc.12 | ABI/type boundary. Keep project adapters on 0.16 until an explicit migration; avoid mixed public types. |
| `ureq` | 2.12, 3.3 | model download; ORT build script | Build/runtime separation. Keep; ORT owns build-time v3. |
| `webpki-roots` | 0.26, 1.0 | `ureq` v2/v3 TLS | Security-sensitive and tied to HTTP stacks. Keep. |
| `imagesize`, `itertools`, `nom`, `weezl` | two generations each | SVG/image/tokenizer/PDF stacks | Transitive, format-specific stacks. Keep. |

The direct project versions of `base64`, `sha2`, and `ndarray` are internally
consistent. Future convergence should be performed with upstream upgrades and
the full model, PDF, TLS, and serialization gates, not with `[patch]` aliases.

