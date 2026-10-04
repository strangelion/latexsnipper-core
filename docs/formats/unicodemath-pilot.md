# UnicodeMath bounded parser/AST pilot

Status: experimental Rust module, 2026-10-04. Not a registered conversion input
or output, full UnicodeMath/Word implementation, or visual-fidelity claim.

Reference: [Unicode Technical Note 28, version 3.3](https://www.unicode.org/notes/tn28/UTN28-PlainTextMath-v3.3.pdf),
sections 2.1–2.3, 3.6, 3.9 and 3.18. The implementation and fixtures are
repository-authored; no renderer/parser source or dependency is vendored.

## Finite syntax and mapping

- ASCII letters/digits and the explicit Greek/symbol inventories in
  `unicodemath_pilot.rs`; alphabetic runs consist of separate variable nodes.
  Digit runs with one internal period/comma are numeric Text nodes.
- Fractions consume adjacent factors, stop at operators/blanks, and associate
  left-to-right. `abc/d` includes all `abc` in the numerator; `a/b/c` nests left.
- Ordinary letter-run script bases use the final letter; script operands may
  contain a run. Same-type script chains nest right; mixed `_`/`^` attach to one
  base. Numeric runs remain atomic in this finite pilot. Compound ungrouped
  script operands and mixed repeated chains require explicit grouping.
- Matched `()`, `[]`, `{}` are visible Delimited nodes; `〖〗` is invisible Group.
  Only outer round parentheses are consumed for fraction/script operands.
- `√`, `∛`, `∜`, and `√(index&radicand)` map to SquareRoot. An unparenthesized
  radical consumes an operand including its scripts; parenthesized radicals
  end at the closing parenthesis.
- `■`, `⒨`, `ⓢ`, `Ⓢ`, `⒱`, `⒩` map to the six corresponding Matrix environments.
  `&` separates cells and `@` separates rows. Nested matrices and explicit empty
  cells are retained; short rows are padded with empty cells, with a work limit.
  `■()` is a one-cell empty matrix, not an empty formula.

This is a separate lexer/parser, not AsciiMath keyword substitution. The only
shared pilot helper compares AST structure. Parsing ASCII spaces explicitly
avoids changing `a b/c` into `ab/c`; redundant spaces otherwise normalize.
No typography/space-width, source-spelling or rich-text buildup parity is implied.

## API, limits and diagnostics

Experimental calls:
`latexsnipper_conversion::unicodemath_pilot::{parse_unicodemath, write_unicodemath}`.
They use the existing `LatexNode` without new variants. Errors expose
UnexpectedToken, UnsupportedSyntax or LimitExceeded, a message and UTF-8 byte
offset. Writer offsets are zero or refer to emitted text, not original LaTeX.
No partial AST, placeholder replacement or global-state mutation is returned.

Writing canonicalizes external Text token boundaries, encloses script bases,
preserves visible delimiters, and reparses output before returning it. The
structural comparison preserves operand ownership, Greek/symbol identity and
matrix row/cell boundaries; it ignores invisible wrappers and Text segmentation.
It is not a theorem-equivalence or layout check. Unrepresentable AST nodes and
ragged external Matrix ASTs fail rather than being silently padded by the writer.
Padding applies only to accepted UnicodeMath source parsing.

Budgets: input/output 64 KiB, lexical input 4,096 tokens, AST 8,192 nodes,
matrix padded area 4,096 cells and depth below 64. Expression/group/command
levels also count, so this is not a promise of 63 nested brackets. Iterative AST
validation bounds visited **and queued** nodes before structural comparison;
joins check output size incrementally. An input near a parse budget can exceed
the emitted grouping/token budget and be rejected by the writer.

Outside this milestone: named function application, n-ary operand semantics,
autocorrect/control words, mathematical styled alphanumerics, combining accents,
quoted text, non-ASCII whitespace, rich-text style, equation arrays/numbering,
skewed/linear fractions and other unlisted glyphs. Unknown glyphs/escapes fail,
not a guessed macro. Decimal and punctuation heuristics are finite, not the full
UTN 28 algorithm; external font runs and arbitrary prose are not accepted.

## Evidence and next gates

Thirteen focused tests include a versioned synthetic fixture (16 fixed accepted
outputs/root kinds, 8 fixed rejection cases), binding and whitespace differences
from AsciiMath, six matrix types, roots/scripts/nested cells, UTF-8 diagnostics,
input/AST/padding limits, failure isolation and 600 deterministic input
combinations. Accepted generated inputs must serialize and canonically reparse.
This is not external accuracy data, random fuzzing or full grammar coverage.

One formula-matrix bridge through existing LaTeX serializers checks MathML/OMML
row/cell/fraction/root/script structure; it does not validate every token in each
exporter or any actual Word rendering. AsciiMath's 12 regressions also pass after
extracting the unchanged private comparison into `pilot_ast.rs`.

Still open: real corpus, external/host comparison, style and spacing loss,
original-source spans, broader syntax, target-specific diagnostics, registered
input/output directions and Rust/C/Python/WASM-JS/Office exposure. Both pilots
remain unavailable in the production registry. OutputFormat, C ABI, binding
envelopes, dependency versions and lockfile are unchanged. FMT-05 remains partial.

Reproduce:

```text
cargo test --locked -p latexsnipper-conversion --no-default-features --test unicodemath_pilot --test asciimath_pilot
cargo test --locked -p latexsnipper-conversion --no-default-features
cargo test --locked -p latexsnipper-conversion
cargo clippy --locked -p latexsnipper-conversion --all-targets -- -D warnings
cargo clippy --locked -p latexsnipper-conversion --no-default-features --all-targets -- -D warnings
cargo check --locked -p latexsnipper-conversion --no-default-features --target wasm32-unknown-unknown
```

Local checks passed: 344 light / 395 native conversion tests including doc tests,
both Clippy configurations, WASM compilation and default workspace tests.
Existing ignored environment-dependent tests were not executed; actual host/visual
tests were not run for this pilot. The reviewed freeze update retains all 28 files and 19
source trees and changes only the registry file and conversion tree hashes.
