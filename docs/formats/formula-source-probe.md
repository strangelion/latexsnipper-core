# Explicit formula source metadata inspection

Experimental native/WASM-portable Rust module `formula_source_probe`, 2026-10-07.
This is source inspection, not a new registered import format or producer adapter.

`probe_text_metadata(key, value)` recognizes only `latex`, `mathml`, `omml`
(ASCII case insensitive). Unknown fields, including descriptions/alternative
text, are not guessed. LaTeX is retained exactly as declared source; syntax and
semantic validation remain the converter's responsibility.

`inspect_formula_xml(xml)` accepts a standalone namespaced MathML `math` or OMML
`oMath`/`oMathPara` and retains the exact complete XML. Prefixes may vary, but the
namespace must match. MathML `math/semantics/annotation` with `application/x-tex`,
`application/x-latex` or `text/x-tex` encoding is decoded separately, retaining
whitespace, CDATA, TeX spelling and the annotation's raw XML byte span. These are
the explicit finite encodings supported here, not a claim about every producer.
Different annotation spellings are marked as conflicting; none is selected
automatically. Unsupported encodings remain in the original XML.

Limits: 256 KiB UTF-8 source, XML 1.0 depth 64, 8,192 reader events and 16 explicitly
TeX-encoded annotations including empty ones. DTD, processing instructions,
unknown entities, malformed framing, unknown namespaces, non-text annotation
markup and declared-format mismatch are rejected. Framing success does not
establish mathematical equivalence, full schema validity or conversion support.

The caller must first establish exact object/carrier association and provenance,
retain input on rejection, and preserve macros/font/style context separately.
No binary substring search, PNG/XMP/SVG metadata extraction, ZIP/CFB stream
extraction, OLE activation, OCR, network/file access, rendering or API/UI binding
is performed. Header class inspection in Office does not implement MTEF import.

Seven authored tests cover explicit keys, source preservation, namespace/format
checks, encoded annotations/conflicts, dangerous/malformed XML, budgets and a
simple fraction handoff to the existing finite MathML/OMML converters. They are
not independent producer fixtures, fuzzing or general import-accuracy evidence.
