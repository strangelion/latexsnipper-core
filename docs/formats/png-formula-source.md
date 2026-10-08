# PNG formula source metadata

Experimental Rust module `png_formula_source`, 2026-10-08. Available to native
and WASM builds. The native `DocumentImporter` attaches candidates to imported
PNG assets; a dedicated binding and the Office selection UI are still pending.

`inspect_png_formula_sources(bytes)` reads explicitly named `latex`, `mathml`
and `omml` fields in tEXt, zTXt and iTXt chunks. These case-insensitive names
are this adapter's finite application convention, not standard PNG formula
keywords or a claim of compatibility with every formula producer.

Text carriers follow the [W3C PNG text chunk specification](https://www.w3.org/TR/png-3/#11textinfo):
tEXt/zTXt are decoded as Latin-1; iTXt as UTF-8, including compressed text.
The adapter keeps keyword spelling, language, translated keyword, compression
flag, source text and the original whole-chunk byte range. MathML/OMML fields
use the existing [formula source inspector](formula-source-probe.md); MathML
TeX annotations retain their decoded text and source XML ranges. The caller
must retain the original PNG and bind the report to the selected object.

Multiple fields remain separate. Different exact source spellings within a
format are marked as conflicts, including explicit LaTeX versus MathML TeX
annotations. Cross-format semantic equivalence is not inferred. Metadata can
be stale or forged; its presence does not prove that it matches the pixels.
No source is selected, executed, rendered or written to a document automatically.

Limits are 16 MiB input, 4,096 chunks, 64 text chunks, 16 recognized fields,
1 MiB total encoded text metadata and 1 MiB total decoded source text. Each
source is limited to 256 KiB of UTF-8; compressed output is bounded before
source parsing. A zlib stream must finish with a valid checksum and no trailing
bytes. All PNG chunk CRCs, basic IHDR/PLTE/IDAT/IEND ordering and framing are
checked. Unknown critical chunks are rejected. Pixel data is never inflated;
successful metadata inspection is not a complete PNG/image validation.

Unknown text keys (including Description, Comment and XMP) are counted and
skipped after keyword validation, without parsing or inflating their bodies.
Their text validity and ancillary semantics are not certified. iTXt language
tags are retained after an ASCII-character check, without a BCP47 registry
lookup. Malformed recognized fields or exceeded limits reject the whole report;
callers retain the image and report the error rather than selecting partial data.

The native importer preserves the exact image as an asset and adds experimental
`asset.metadata.formula_source_candidates_v1` only when source candidates exist.
This versioned object includes `carrierSha256` for the original PNG bytes,
`sourceKind: declared-metadata`, ordered sources, byte ranges, annotations and
conflicting formats. It does not synthesize formula blocks or trigger OCR.
Conflicts produce `W_PNG_FORMULA_SOURCE_CONFLICT`; rejected metadata produces
`W_PNG_FORMULA_SOURCE_REJECTED`, keeping the source image import intact within
the normal import limits. Metadata-free images keep their previous behavior.
Consumers must still verify image/object identity and obtain source selection
before conversion. These candidate fields do not advertise PNG-to-formula
conversion support in the capability registry.

Ten portable authored tests cover Unicode/Latin-1, both compressed carriers, XML source
handoff, provenance, exact duplicate/conflict handling, after-IDAT text, every
truncated prefix of a small fixture, CRC/zlib corruption and budgets. Fixtures
are generated from an authored one-pixel grayscale PNG; these are not real
third-party producer compatibility or OCR accuracy results. A native integration
test checks source image byte preservation, candidate identity/hash, conflicts,
rejected metadata and absence of automatically generated formula blocks.

Remaining: SVG/XMP producer profiles, public binding and host selection wiring,
real producer samples with clear provenance, and optional OCR for pixel-only
images. MTEF/CFB/OLE support is separate.
