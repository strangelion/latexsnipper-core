# FormulaLayout / Math Semantic IR Audit

## Decision

`latexsnipper_ast::FormulaLayout` remains the only structural mathematical
tree. The project must not introduce a parallel `MathNode` hierarchy.

`FormulaSource` remains the loss-aware source representation, and future
semantic information must be additive annotations attached to `FormulaLayout`.
The `Document` 1.0.0 wire schema is not changed by this audit.

## Current Coverage

| Concern | Existing representation | Current producer | Current consumer | Gap |
| --- | --- | --- | --- | --- |
| Symbols and categories | `FormulaNode::Symbol` | `formula_parser` | layout inspection | no canonical symbol normalization |
| Groups, superscripts, subscripts | `Group`, `Superscript`, `Subscript` | `formula_parser` | layout inspection | no cross-format parser parity |
| Fractions and roots | `Fraction`, `SquareRoot` | `formula_parser` | layout inspection | no layout-first exporter |
| Commands and functions | `Command` and `SymbolCategory::Function` | `formula_parser` | layout inspection | function-call semantics are implicit |
| Relations, accents, fences, n-ary operators | command/symbol categories | `formula_parser` | layout inspection | no dedicated semantic annotation |
| Matrices and cases | `Environment` | `formula_parser` | layout inspection | row/column and cases semantics are implicit |
| LaTeX source | `FormulaSource::Latex` | syntax and recognition | all converters | layout is not attached by syntax parsing |
| OMML / MathML / Typst source | `FormulaSource` variants | importers | format-specific converters | no parser into `FormulaLayout` |

## Evidence in the Current Code

- `crates/inference/src/formula_parser.rs` already parses LaTeX into
  `FormulaLayout`, including fractions, roots, scripts, commands, and
  environments.
- `FormulaLayoutNode` is the current pipeline integration point for that
  parser.
- `crates/syntax/src/latex.rs` deliberately preserves source spans and stable
  IDs, but does not populate `Formula.layout`.
- Conversion and export paths currently dispatch mostly from `FormulaSource`.
  They do not yet treat `FormulaLayout` as the canonical multi-format input.

## Approved Evolution Order

1. Add a non-breaking semantic annotation layer to `FormulaLayout`; do not add
   a second structural tree. **Implemented:** optional `SemanticAnnotation`
   records refer to producer-defined node paths and are omitted when empty.
2. Define canonical normalization over the existing node variants.
3. Make LaTeX parsing able to opt into layout creation without changing the
   default source-aware parsing contract.
4. Add OMML, MathML, and Typst import adapters that produce the same layout.
5. Move multi-format conversion to prefer normalized layout when available,
   retaining source-specific fallback and fidelity diagnostics.
6. Add round-trip corpus cases before declaring any source format canonical.

## Compatibility Rules

- New semantic metadata must be optional and skipped when empty.
- Existing `FormulaSource` content must remain available for lossless fallback.
- New layout variants require round-trip fixtures for LaTeX, OMML, MathML, and
  Typst before becoming preferred conversion inputs.
- An annotation must describe an existing `FormulaLayout` node; it must never
  duplicate the tree structure.
