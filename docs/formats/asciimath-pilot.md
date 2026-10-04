# AsciiMath bounded parser/AST pilot

Status: experimental Rust module, 2026-10-04. This is not a registered semantic
conversion target, a full AsciiMath implementation, or a visual fidelity claim.
UnicodeMath has a different grammar and is not implemented by this module.

The grammar reference is [the AsciiMath project's syntax documentation](https://asciimath.org/#syntax).
The implementation is repository-authored Rust; it does not embed a JavaScript
renderer or vendor source. The versioned fixture is repository-authored synthetic
data at `crates/conversion/tests/fixtures/asciimath-pilot-v1.json`, not an external
accuracy corpus. No new dependency or model is required.

## Accepted syntax and AST mapping

| Input category | Pilot mapping / rule |
| --- | --- |
| ASCII letters and decimal numbers | `LatexNode::Text`; letters are single variables unless a longest-match keyword applies. Whitespace separates tokens, not arbitrary prose. |
| Greek names, common operators/functions, listed relation/symbol aliases | Greek, Operator, zero-argument function Command or Symbol; the finite token lists are in `asciimath_pilot.rs`. |
| `CC`, `NN`, `QQ`, `RR`, `ZZ` | `mathbb` FontModifier constants; not two plain variables. |
| Matched round/square/curly brackets | Visible Delimited nodes; unmatched or asymmetric pairs fail. |
| `{: ... :}` | Invisible Group, including compound script bases. |
| `/` and `frac` | Fraction with exactly two operands; outer operand brackets are consumed as grouping. `/` binds intermediate expressions, not an entire surrounding sum. Chained division requires explicit grouping. |
| `sqrt`, `root` | SquareRoot, with the root index retained separately. Unary/binary commands consume simple operands. |
| `_` followed optionally by `^` | Subscript/Superscript with an explicit base; repeats or reversed order require grouping. |
| `abs`, `norm`, `floor`, `ceil` | Explicit visible delimiter pairs, not stripped text. |
| Uniform square/square or round/round nested row notation | `bmatrix` / `pmatrix`, explicit rows/cells, including one-cell/column and nested matrices. Row widths must agree; empty cells fail. |

The finite grammar follows the reference's simple/intermediate/expression levels,
not arithmetic evaluation or global string substitution. Examples:

- `a+b/c+d` has a fraction only at `b/c`.
- `(a+b)/(c+d)` has grouped numerator and denominator with no added visible brackets.
- `sqrt x^2` attaches the superscript to the root, as a simple-expression command.
- Matrices retain their row/cell structure; augmented matrices and layout/cases
  notation are outside this pilot.

## Calls and failure behavior

Use `latexsnipper_conversion::asciimath_pilot::{parse_asciimath, write_asciimath}`.
Parsing returns the existing `LatexNode` or `AsciiMathError`. The error exposes
UnexpectedToken, UnsupportedSyntax, RaggedMatrix or LimitExceeded, a message and
the UTF-8 byte offset. Output-side errors may use offset zero or the emitted
text's offset, not an original LaTeX source span. No partial AST is returned;
callers retain their original source. A failure does not alter global state or
make later calls fail.

Writing supports only representable AST nodes and produces canonical AsciiMath.
It separates variable letters to avoid accidentally turning them into keywords,
groups compound script bases invisibly, and checks the reparsed structure before
returning success. The check retains operand ownership, visible delimiters,
matrix rows/cells and supported command identity; it ignores invisible grouping
and sequence wrappers. It is not a proof of mathematical equivalence, layout,
font matching, or original-source identity. Ambiguous ASTs that would become a
different matrix/delimiter structure fail instead of being silently relabeled.

Input and output are bounded to 64 KiB. Lexing allows at most 4,096 tokens;
parsing/writing allow at most 64 recursive expression/node levels, and writing
has a 4,096-node budget. These are conservative work limits, not equal limits
on bracket count: command/group levels also consume depth. Joins are checked
incrementally to avoid building a large output before rejecting it. Structural
comparison uses a bounded tree, not recursively escaped diagnostic strings.

Unsupported text/quoted strings, accents, general font commands, colors, cases,
augmented matrices, unmatched/asymmetric brackets, LaTeX escapes, backtick
document wrappers and other unlisted constructs fail. Unrecognized ordinary
letters are variables, not custom function names or macros. The known unsupported
keyword list prevents those constructs from being read as plain variable runs.
No recovery that discards content or replaces it with a placeholder is used.

## Verification and remaining gates

The 12 focused tests cover fixed source/expected-root fixtures, numerator and
denominator ownership, script order/base binding, roots, symbol longest matching,
decimal/whitespace handling, matrices and nested cells, unsupported/malformed
input, limits and failure isolation. Another deterministic test exercises 4,913
three-token combinations; accepted ASTs must serialize and canonically reparse.
It is not fuzzing coverage of arbitrary inputs or an accuracy percentage.

One structure regression maps a parsed nested formula matrix through the existing
LaTeX serializers to MathML/OMML and checks row/cell, fraction, root and script
nodes. This does not validate every pilot token in every exporter or in Word.

Still open: UnicodeMath's independent pilot; expanded valid/invalid real corpus;
external renderer/host comparisons; typography, whitespace visual fidelity and
source spans on AST nodes; selected-target loss reporting; registered input/output
directions; Rust/C/Python/WASM-JS API and Office UI exposure with corresponding
acceptance. FMT-05 is not closed by this pilot. The registry keeps `ascii-math`
unavailable and states that its parser is experimental; OutputFormat and existing
binding envelopes/ABI are unchanged.

Reproduce without native runtime dependencies:

Local checks passed on 2026-10-04: 331 light / 382 native conversion tests
(including doc tests), both Clippy configurations, WASM compilation and the
default workspace tests. Existing ignored environment-dependent tests were not
executed. Frozen contracts retain all 28 files and 19 source trees; only the
reviewed capability-registry file and conversion tree hashes changed.

```text
cargo test --locked -p latexsnipper-conversion --no-default-features --test asciimath_pilot
cargo test --locked -p latexsnipper-conversion --no-default-features
cargo clippy --locked -p latexsnipper-conversion --no-default-features --all-targets -- -D warnings
cargo check --locked -p latexsnipper-conversion --no-default-features --target wasm32-unknown-unknown
```
