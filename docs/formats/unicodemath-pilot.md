# UnicodeMath bounded parser/AST pilot

Status: finite Rust grammar with formal formula-only routes, 2026-10-13.
The pilot module name remains compatible; see [the registered binding contract](linear-formula-conversion.md).
This is not a full UnicodeMath/Word implementation or visual-fidelity claim.

Reference: [Unicode Technical Note 28, version 3.3](https://www.unicode.org/notes/tn28/UTN28-PlainTextMath-v3.3.pdf),
sections 2.1–2.3, 3.4–3.6, 3.9, 3.15 and 3.18. The implementation and fixtures are
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
- ASCII-double-quoted ordinary text maps to one `text` Command with a literal
  Text argument, not a run of mathematical variables. Spaces, Unicode prose and
  mathematical-looking punctuation inside it remain text. `\"` represents a
  literal quote; the finite canonical writer also uses `\\` for a literal
  backslash. Other backslashes are text, not control-word execution. Empty text,
  quoted fraction operands, matrix cells and script bases/operands are retained.
  Control characters, line breaks and nested mathematical AST inside text fail.
- The explicit 21-name function inventory includes trigonometric functions,
  `log`, `ln`, `exp`, `lim`, `min`, `max`, `det`, `dim` and `gcd`. Names are upright
  Operator nodes, not variable-letter runs. Application accepts U+2061, a blank
  or a following grouping. A bracketed argument ends at its closing bracket;
  `sin(x)b` keeps `b` outside, while `sin⁡〖(x)b〗` keeps it inside. Ungrouped
  operand/fraction arguments use the finite binding rules above. Canonical calls
  use U+2061 plus invisible argument grouping. Function-head scripts and scripts
  immediately after U+2061 are supported; already-scripted heads followed by
  more scripts after U+2061 require explicit grouping. Bare known names remain
  operators. Unknown names with U+2061, aliases and substring guessing fail.
- Nine n-ary glyphs (`∫`, `∬`, `∭`, `∮`, `∑`, `∏`, `∐`, `⋃`, `⋂`) use Operator
  heads with at most one lower and one upper limit. A blank or `▒` binds the
  following operand; canonical bodies use `▒〖...〗`. Bracketed bodies retain
  their brackets and stop there, while invisible grouping can include adjacent
  factors. `▒` also works as the finite function-apply alias. Bare heads remain
  operators; explicit markers require nonempty bodies. Numeric flag `0` uses
  the default; `1` and `2` select under-over and side-script placement when the
  complete head has at least one limit or placeholder. Canonical text retains these overrides;
  LaTeX uses `\limits`/`\nolimits`, and OMML/MathML retain nondefault placement
  on readback. XML properties equal to the finite default may normalize to that
  default, not the original flag spelling. Growth flags `64`/`128` and their
  combinations with `1`/`2` are preserved in the bounded AST and structured
  OMML path; see below. Other numeric flags, duplicate limit
  slots, unknown glyphs and stray markers fail.
  The differential glyph `ⅆ` is retained rather than replaced by ASCII `d`;
  its font and differential spacing are not a host-fidelity claim.

For example, `"rate"="distance"/"time"` becomes a text-labelled fraction, and
the corresponding LaTeX text commands can serialize back to quoted UnicodeMath.
TeX-significant text characters are escaped before constructing a LaTeX bridge;
braces/comments/separators and command-looking prose cannot become active TeX
syntax. MathML/OMML projection decodes those escapes into literal text and applies
XML escaping. This is plain text, not rich-text/font parity or actual Word buildup.

This is a separate lexer/parser, not AsciiMath keyword substitution. The only
shared pilot helper compares AST structure. Parsing ASCII spaces explicitly
avoids changing `a b/c` into `ab/c`; redundant spaces otherwise normalize.

## Structured n-ary growth

The finite masks combine one placement (`0`, `1`, `2`), optional empty slots
(`0`, `8`, `16`, `24`), and one growth choice (`0`, `64`, `128`).
`64` prevents operand-height growth and `128` enables it; either combines with
the existing placement and placeholder bits. Both growth bits together and
other placement modes are rejected, not ignored. Placement still requires a slot;
growth alone does not. The meanings follow [UTN 28 section 3.4](https://www.unicode.org/notes/tn28/UTN28-PlainTextMath-v3.3.pdf).

UnicodeMath/AsciiMath to OMML now serializes the validated AST directly rather
than reparsing generated TeX. Growth annotations use reserved internal Command
nodes without adding a public enum variant or a source macro. The OMML linear
adapter reconstructs these annotations; its private intermediate marker cannot
be supplied in XML text or escape into exported content. The AST Display view
is diagnostic for growth, not a LaTeX exporter. Formal routes to LaTeX, MathML,
Typst, Markdown and HTML refuse growth rather than manufacture a macro or omit it.

OMML to OMML also uses this structured path. Omitted n-ary `grow` means false,
whereas a present `grow` with no `val` means true; see [Microsoft's definition](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.math.growoperators?view=openxml-3.0.1).
The adapter keeps no-growth even on bare heads, nested operands and matrices.
Explicit true can normalize to the existing Core default on XML readback;
this is not preservation of the original flag spelling. Unknown, empty explicit
or duplicate property values fail. The general public XML-to-LaTeX reader cannot
export explicit no-growth faithfully through this pipeline and reports that limit.
Other target mappings and actual Word font/height fidelity remain independent work.

## Empty limit slots

`8` requests a lower placeholder, `16` an upper placeholder and `24` both.
They become empty Subscript/Superscript arguments in the existing AST, not a
new enum, zero, dummy variable or source macro. Canonical UnicodeMath uses the
placeholder bits instead of `_()`/`^()`; only n-ary script slots accept that
empty spelling. Empty ordinary script operands or n-aryands remain rejected.
Real limits replace empty slots; redundant empty slots cannot erase a real limit,
and two nonempty limits in the same slot still fail. A filled placeholder bit
therefore normalizes away rather than preserving its original spelling.

OMML explicitly emits `subHide=0`/`supHide=0` for empty visible slots and `1`
for absent limits. Readback distinguishes an encoded empty argument, an absent
argument and a hidden argument. Omitted hide properties mean visible; a present
hide property without `val` means hidden. Invalid, duplicate, empty explicit or
non-leaf boolean properties fail. Hidden nonempty limits are not rewritten as
ordinary visible scripts: [Word can relocate their content](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/21558c72-3684-46bd-99ec-146aef4a5a05).
MathML/LaTeX keep empty slot structure, not Word's editable placeholder appearance;
actual host/UI and font/layout verification remain separate.
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
For UnicodeMath only, ASCII blanks in bare mathematical Text nodes normalize;
quoted text arguments retain their blanks. AsciiMath's comparison is unchanged.
It is not a theorem-equivalence or layout check. Unrepresentable AST nodes and
ragged external Matrix ASTs fail rather than being silently padded by the writer.
Padding applies only to accepted UnicodeMath source parsing.

Budgets: input/output 64 KiB, lexical input 4,096 tokens, AST 8,192 nodes,
matrix padded area 4,096 cells and depth below 64. Expression/group/command
levels also count, so this is not a promise of 63 nested brackets. Iterative AST
validation bounds visited **and queued** nodes before structural comparison;
command arguments are also visited. Quoted content and its escapes count toward
the lexical-work budget even though a whole quoted span is one parser token.
Escaped LaTeX text and emitted quoted output must separately fit 64 KiB;
joins check output size incrementally. An input near a parse budget can exceed
the emitted grouping/token budget and be rejected by the writer.

Outside this milestone: arbitrary function/n-ary names, other n-ary numeric flags,
placement on heads without slots, growth in other semantic targets and inherited host layout,
autocorrect/control words, mathematical styled alphanumerics, combining accents,
unquoted non-ASCII whitespace, rich-text style, equation arrays/numbering,
skewed/linear fractions and other unlisted glyphs. Unknown glyphs/escapes fail,
not a guessed macro. Decimal and punctuation heuristics are finite, not the full
UTN 28 algorithm; external font runs and arbitrary prose are not accepted.

## Evidence and next gates

Twenty-three focused tests include a versioned synthetic fixture (16 fixed accepted
outputs/root kinds, 8 fixed rejection cases), binding and whitespace differences
from AsciiMath, six matrix types, roots/scripts/nested cells, UTF-8 diagnostics,
input/AST/padding limits, failure isolation and 600 deterministic input
combinations. New quoted-text tests independently check expected LaTeX/canonical
output, empty/multilingual text, XML/TeX protection, script/matrix ownership,
UTF-8 error offsets and lexical/escaped-output budgets. Formal registry cases
check input hashes, explicit best-effort mode and existing target boundaries.
The C envelope is exercised directly; the new WASM test is compiled, with its
browser execution a separate gate. Accepted generated inputs must serialize and canonically reparse.
This is not external accuracy data, random fuzzing or full grammar coverage.

Function regressions check all supported names in MathML/OMML projection, fixed
canonical arguments/scripts, nested calls, missing/unknown applications and depth
limits. The LaTeX bridge now retains nested Group nodes instead of flattening
function arguments or script bases. Plain brace nesting uses a bounded iterative
stack; over-budget source is literal in the legacy parser and strictly refused.
Three separate regressions check ownership, escaped/comment braces and recovery.
Finite XML readback checks mathematical arguments, visible brackets and scripted
heads. MathML U+2061 requires a known upright name and one in-scope argument;
unknown/stray markers or ungrouped scalable-fence arguments are refused.
OMML normal runs cannot identify the original quoted-text AST: that reconstruction
is not claimed as a quoted-text round trip. Limit layout and actual host typography
still require their existing independent gates.

N-ary regressions check all nine glyphs, fixed canonical limits/body scope,
bracket boundaries, nested operators/functions, differential identity and finite
XML readback. MathML uses explicit large-operator/default-limit properties;
its finite two-child row projection retains an owned body. Upper/lower slot
ordering normalizes independently of source spelling, without erasing the limits.
Bare mathematical text is tokenized without treating a variable run as an upright
word; quoted text is separate. Style declarations retain their existing scope
and inherited display mode. OMML rejects unknown explicit n-ary characters and
duplicated properties; its existing omitted-character default is unchanged.
The best-effort routes and reconstruction/style warnings remain mandatory.

One formula-matrix bridge through existing LaTeX serializers checks MathML/OMML
row/cell/fraction/root/script structure; it does not validate every token in each
exporter or any actual Word rendering. AsciiMath's 12 regressions also pass after
extracting the unchanged private comparison into `pilot_ast.rs`.

Still open: real corpus, external/host comparison, style and spacing loss,
original-source spans, broader syntax, richer target-specific diagnostics and Office
exposure. Registered input/output directions and Rust/C/Python/WASM-JS formula bindings
now have finite best-effort formula routes in the registry. Legacy document OutputFormat,
C ABI, binding envelopes, dependency versions and lockfile are unchanged. FMT-05 remains partial;
Office UI, full grammar and true Word buildup comparisons remain open.

Reproduce:

```text
cargo test --locked -p latexsnipper-conversion --no-default-features --test unicodemath_pilot --test asciimath_pilot
cargo test --locked -p latexsnipper-conversion --no-default-features
cargo test --locked -p latexsnipper-conversion
cargo clippy --locked -p latexsnipper-conversion --all-targets -- -D warnings
cargo clippy --locked -p latexsnipper-conversion --no-default-features --all-targets -- -D warnings
cargo check --locked -p latexsnipper-conversion --no-default-features --target wasm32-unknown-unknown
```

Local checks on 2026-10-13 passed: light and native conversion suites including
doc tests, both Clippy configurations, workspace compilation, the selected C
envelope regression and conversion-only WASM test compilation. Native filesystem
fixtures use the existing repository-owned test temporary directory; no global
permission changes are required. Ignored environment-dependent tests and actual
host/visual tests were not run. The freeze update retains all 35 contract files
and 21 source trees; this batch changes only conversion and FFI source-tree hashes.
