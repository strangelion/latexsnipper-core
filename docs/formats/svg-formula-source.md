# SVG formula source metadata

Experimental Rust module `svg_formula_source`, 2026-10-08. Native imports attach
source candidates to the original SVG asset. Dedicated bindings and the Word
SVG selection viewer are pending; PNG's Word entry is separate.
An authored fixture in x64 Word 16.0.18526 lost its formula metadata during
`AddPicture`, in saved DOCX bytes and after reopening. Word's SVG extension and
PNG fallback have separate relationships; Office now rejects the fallback in
its PNG source reader. Host wiring must retain an original source asset or
report it unavailable, not claim that normalized Word XML is the original SVG.
This observation does not establish behavior for every Word version/producer.
Office now retains app-managed SVG/source payloads in its document manifest
and hash-binds them to the actual Word SVG carrier on selection/ID readback.
This is not a generic third-party metadata viewer. In-place managed SVG image
updates remain blocked before mutation while the Word SDT boundary is repaired.

`inspect_svg_formula_sources(svg)` accepts a bounded, namespaced standalone SVG
XML document. It inspects only direct children of top-level SVG `metadata`:

- Namespaced MathML `math`, including its explicit TeX annotations.
- Namespaced OMML `oMath` or `oMathPara`.
- `<fs:source format="latex">text or CDATA</fs:source>` where `fs` is bound to
  `urn:latexsnipper:formula-source:v1`. This is an explicit application profile,
  not a claim that arbitrary SVG producers use this namespace or format.

The metadata carrier follows [SVG 2's metadata element](https://www.w3.org/TR/SVG2/struct.html#MetadataElement).
Title/description strings, nested object metadata, RDF/XMP and unknown fields
are not guessed as formulas. Unknown direct metadata children are counted.
Multiple supported fields stay separate and conflicting exact spellings are
reported, including explicit LaTeX versus MathML TeX annotations. Cross-format
equivalence and correspondence to rendered graphics are not inferred.

The report retains the original SVG and exact whole-element UTF-8 byte ranges.
For mathematical XML, inherited namespace bindings are materialized on a
standalone root before the existing [formula inspector](formula-source-probe.md)
validates framing and extracts TeX annotations. `namespace_materialized` records
this step; the original fragment remains recoverable from the SVG range.
Annotation ranges refer to the standalone `source`, not the carrier bytes.
The inspector does not reconstruct math, execute scripts, fetch external
resources, render the SVG, or establish schema validity. It is not an SVG
sanitizer or evidence of support by the finite mathematical converters.

Limits: 4 MiB UTF-8 SVG input, depth 64, 65,536 XML events, 64 attributes per
carrier element, 64 effective namespace bindings at a mathematical root,
16 recognized fields, 256 KiB per raw field/standalone source, and 1 MiB total
source text. DTD, processing instructions, undeclared namespaces, unknown
entities, duplicate expanded attribute names and malformed framing are rejected.
Only XML 1.0 with implicit or explicit UTF-8 is accepted. Supported namespace
URIs use their literal declared spelling. A malformed recognized field rejects
the report; callers preserve the original carrier and display a diagnostic.

The native `DocumentImporter` writes experimental
`asset.metadata.formula_source_candidates_v1` with `carrier: svg`, the original
`carrierSha256`, `sourceKind: declared-metadata`, ordered sources, XML ranges,
namespace materialization flags, annotations and conflicting formats. Conflicts
produce `W_SVG_FORMULA_SOURCE_CONFLICT`; rejected metadata produces
`W_SVG_FORMULA_SOURCE_REJECTED`, preserving the asset within normal import limits.
No formula block or automatic source selection is created by metadata inspection.
The basic SVG shape reader now skips metadata subtrees, keeping their nested
text/shape names out of visible drawing primitives.

Ten portable authored tests cover finite fields, Unicode/CDATA, inherited and
overridden namespaces, namespace quoting, standalone XML, conflicts, ignored
descriptions/XMP, malformed input, all truncated prefixes of a small fixture,
budgets and non-rendering metadata. One native test checks original asset bytes,
hash-bound candidates, conflicts and rejection diagnostics. These are authored
fixtures, not third-party producer compatibility or general import accuracy.

Remaining: explicit XMP/producer profiles, host selection wiring, real producer
samples with provenance, and source selection before conversion. MTEF/CFB and
OCR remain separate paths.
