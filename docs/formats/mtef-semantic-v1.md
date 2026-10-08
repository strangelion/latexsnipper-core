# Experimental MTEF v5 finite semantic profile

Updated: 2026-10-09. This is an explicit Rust-only read path, not registered
MathType import, visual fidelity, third-party OLE activation or a MTEF writer.

`mtef_semantic::read_mtef_v5(bytes)` returns the unchanged borrowed framing
inspection, optional `LatexNode`/canonical LaTeX, source-offset losses and an
optional error. Any framing/reference/unsupported-structure error returns no
AST and no LaTeX; the original full bytes remain available. Generated LaTeX is
reconstructed, never claimed to be the author's source. Prefer `report.latex`
for command boundaries, rather than assuming generic AST string formatting.
The inline/display bit remains in the inspected header; hosts must choose their
insertion mode explicitly instead of inferring it from the content-only AST.

## Profile v1

- Raw v5 only; known Mac/Windows and MathType/Equation Editor header values,
  product version at least four. One top-level LINE or PILE.
- Explicit ASCII alphanumerics and finite punctuation; listed Unicode MTCode
  Greek/math glyphs. Mathematical typefaces 3-6/8 only. Numeric runs retain a
  compound base; variable scripts attach to the preceding atom.
- Horizontal/slash/piece/baseline fractions retain numerator then denominator.
  Layout variations become a normalized fraction with an explicit layout loss.
- Square/nth roots retain radicand then degree; square-root degree must be NULL,
  nth degree and required operands must be nonempty.
- Postscript selectors 27-29 retain subscript then superscript, including NULL
  placeholders. Orphan, duplicate, mismatched slots and prescripts are rejected.
- Matrices retain row-major dimensions and empty cells. Piles retain lines as
  an `aligned` AST table. Non-LINE slots or mismatched dimensions are rejected.

The glyph inventory is explicit in source; this is not a cast of every 16-bit
value to Unicode. Private-use, surrogate, encoded-only and unmapped characters,
explicit negative fonts, functions, embellishments and other templates remain
unsupported. Definition references must precede use. Fonts, default typography,
colors, spacing, nudges, matrix placement/partitions and encoded font positions
are retained in raw bytes and reported as losses, not silently restored.
ASCII whitespace and U+2212 normalization also have explicit loss information.
There is no arbitrary equation-validity or visual-equivalence verdict.

Budgets retain the raw inspector limits, plus 8,192 AST construction steps,
64 AST levels, 1,024 matrix cells and 64 KiB canonical output. Bounds apply
before returning any AST. Unsupported input has no guessed partial result.

## Validation and remaining work

The versioned fixture is repository-authored synthetic data, not MathType/SDK
sample bytes. Focused tests exercise fixed output, operand/slot ownership,
command boundaries, MathML/OMML handoff for selected structures, losses,
truncated prefixes, mutations and limit edges. Existing large-operator MathML
limit handling remains separate. These checks are not real MathType accuracy,
Office object readback or all-producer compatibility evidence.

The new module is a reviewed additive experimental Rust interface. Existing
framing, input/output wire enums, registry and native/WASM production `mtef`
gates remain unchanged; the conversion-tree freeze is refreshed deliberately.
Next: semantic batch reuse, permitted real samples, explicit font/reference
mapping, additional glyph/templates and bounded container/host extraction.
Only after independent acceptance should this enter the formal registry/UI.

References: [MTEF v5 record specification](https://docs.wiris.com/en_US/mathtype-mtef-v5-mathtype-40-and-later)
and [MTCode/font encoding explanation](https://docs.wiris.com/mathflow/en/sdk/reference/font-knowledge.html).
