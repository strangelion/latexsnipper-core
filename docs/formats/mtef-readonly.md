# MTEF v5 experimental read-only inspection

Status: first bounded Rust byte-inspection milestone, 2026-10-04. This is not
MathType import/export, an AST converter, a renderer, a writer or an OLE server.
Production `mtef` input remains unavailable in the conversion registry/API/UI.

## Reference and asset boundary

The functional record reference is Wiris' public
[MTEF v5 specification](https://docs.wiris.com/en_US/mathtype-mtef-v5-mathtype-40-and-later).
Its raw byte stream is distinct from OLE/file/clipboard storage. Older versions
are not parsed using v5 record rules. Public format documentation is not treated
as clearance to redistribute SDK binaries, source, fonts or third-party files.

`tests/fixtures/mtef-readonly-v1.json` contains repository-authored synthetic
bytes with the application key `pilot`. None were exported by MathType or copied
from its SDK sample equation. They establish reproducible record framing, not
real MathType compatibility. No new dependency, proprietary SDK or sample asset
is included. Actual user-provided/exported fixtures require recorded provenance
and permission before repository distribution; SDK/third-party write licensing
and independent legal review remain open, not approved by this milestone.

## Explicit Rust interface

```rust
use latexsnipper_conversion::mtef_readonly::inspect_mtef_v5;

let report = inspect_mtef_v5(raw_bytes);
// Always retain report.source, including stopped/unknown/truncated input.
// report.complete is framing only, never a conversion or semantic-validity gate.
```

`Inspection` borrows the original complete input without modifying/copying it.
Header key, font/encoding/color names, matrix partition bytes and future payloads
are byte ranges, not assumed UTF-8. Scalars and preference spelling are decoded
for diagnosis. Records are flat preorder entries with depth and half-open spans;
container spans include children and their END, while an incomplete record span
contains only its consumed prefix. `consumed` is not a safe resynchronization or
editing offset. Completed prefix records can coexist with incomplete ancestors.

## Batch inspection and similar record layouts

`mtef_batch::inspect_mtef_v5_batch` adds an experimental in-memory batch interface.
Identical raw bytes share one inspection; each occurrence retains its own original
source slice. Hash-map collisions still require full byte equality. The session
ends with the call/result lifetime; no global cache, object ID reuse or disk
persistence is added. A 10,000-occurrence authored duplicate fixture produces one
inspection, not 10,000 copies of the decoded records.

Known complete streams receive a versioned record-layout key. It includes record
types/depths, template selector/variation/options, matrix dimensions and NULL-line
options, while excluding character values. Fractions with different operands can
therefore be grouped without sharing their decoded operand values. This key is a
similarity hint for planning, never semantic equivalence or permission to reuse a
converted formula. Stopped, future/opaque and empty streams have no layout key.
Font/encoding references and template semantics still require the gates below.

Batch budgets are 10,000 occurrences, 16 MiB aggregate input including duplicates,
65,536 inspected records and 65,536 stored diagnostic issues. Large documents
must be processed in bounded chunks. Excess diagnostics return `BatchLimit::Issues`
instead of silently dropping issues; duplicate occurrences share issue storage.
The single-stream limits continue to apply. These are raw-byte inspection tests,
not a 10,000-formula Word or MathType conversion benchmark. Registered `mtef`
conversion remains disabled.

## Finite reference and slot diagnostics

Updated: 2026-10-09.

`mtef_diagnostics::diagnose_mtef_v5` adds a separate experimental report. It
retains the unchanged framing inspection and byte slice, then checks prior
encoding/font/color definitions and equation-preference font indices. Custom
encoding indices begin at five; unused zero preference entries and omitted
built-in style defaults are not mistaken for missing definitions. Later
definitions do not retroactively resolve earlier references.

Direct LINE slots (including NULL slots) are counted for matrices and selector
10/11 root/fraction templates. Nested lines do not inflate the parent slot count;
non-LINE structural children receive a finite-profile diagnostic. This does not
validate every allowed template variation or mathematical operand identity.
Other template selectors are reported unsupported, not assigned guessed slots.

CHAR records retain all raw MTCode/font positions. Every MTCode is explicitly
reported unmapped; encoded-only records have no resolved character identity.
Negative explicit typefaces remain unsupported pending a reviewed reference
map; no font is loaded and no code is cast to Unicode. Future/empty streams and
incomplete framing remain explicit. Incomplete framing has no partial reference
verdict. Issues identify record index, source offset, field and optional value.
Work/storage is bounded by the existing record and aggregate array budgets.

`BatchInspection::issues(index)` shares this pass for byte-identical sources,
without parsing them twice. Similar layout keys do not share differing MTCode
diagnostics. The existing `get` result, framing semantics and registry remain
unchanged. An empty issue list or a layout key is never a conversion-ready or
semantic-validity verdict. No AST, host extraction or registered MTEF import is
enabled by this addition.

The additional ten authored tests cover definition ordering, built-in/custom
encodings, unused preferences, colors, unresolved character identity, direct vs
nested slots, NULL slots, matrix counts, future/unknown records, all truncated
prefixes of an authored stream, deterministic mutations and oversized input.
Existing batch tests also verify diagnostic sharing only for identical bytes.
These are synthetic diagnostic checks, not real MathType accuracy evidence.

Public experimental Rust changes include the new module/types/function,
`BatchInspection::issues`, `MAX_BATCH_ISSUES` and `BatchLimit::Issues`. Rust callers
exhaustively matching this experimental limit enum must handle the new variant.
Existing framing and conversion wire enums are unchanged. The conversion tree
snapshot is deliberately reviewed/refreshed with
the full frozen contract-file and source-tree inventory preserved.

## Framing covered

- Raw v5 header, product/platform metadata, inline bit and application key.
- Tags 0-19: lines (including NULL slots), characters/embellishment lists,
  template selectors/variations/options and slots, piles/rulers, matrix
  dimensions/partition byte spans/slots, font/style/encoding/color definitions,
  size records and counted equation preference arrays.
- Distinct variable-width unsigned/biased signed integers, fixed little-endian
  16-bit fields, short/extended nudges, 8/16-bit encoded font positions,
  tag-only END/typesize records and nibble-packed dimension source spelling.
- Tags >=100: read the specified variable-width payload length and retain the
  opaque range. Emit `FutureRecord`, even when the surrounding framing completes.
- Unknown unframed tags 20-99 or reserved structural option bits: stop with a
  byte-offset diagnostic. Never guess lengths or scan for an apparent END.
- Truncation, conflicting encoded-position flags, invalid dimension nibble/unit/
  padding, out-of-range color components and bytes after the equation END fail
  framing explicitly. Missing required RULER does not consume the next record.

The general object-list summary in the reference must not be used to add an END
to every tag: detailed RULER fields are a counted list, CHAR has an object list
only under its embellishment flag, and NULL LINE has no list/END.

Budgets: 1 MiB input, 4,096 total records (including END/ruler), depth below 64,
4,096 bytes per zero-terminated string/decoded dimension spelling, and 8,192
aggregate dimension/style/tab-stop entries. Oversized inputs are still borrowed
intact but not parsed. Payload lengths cannot exceed remaining bytes. Bounds
include positive edge tests; no unbounded recovery/recursion is attempted.

## Not validated or supported

`complete` only means the available record fields, nested termination and final
byte boundary were consumed. An END-only stream or a matrix with mismatched
slot counts can frame successfully; neither is thereby a valid equation.
Font/encoding/color definition order and references, alignment/tab/style value
semantics, template-specific option/variation validity and operand slot ordering,
matrix cell counts, header product/version consistency, MTCode-to-Unicode/font
mapping and mathematical meaning are not validated. MTCode is retained as a raw
16-bit code, not blindly cast to Unicode or LaTeX.

No OLE/WMF/document/clipboard extraction, third-party object activation or
overwrite, v0-v4 parsing, recovery rewrite, AST/LaTeX/MathML/OMML conversion,
generation, application automation, registered binding or UI exposure is added.
Future records retain unknown semantics even when their lengths are safe.

## Acceptance evidence and next gates

15 focused tests cover six complete/eight rejected versioned fixtures, every
truncated prefix of complete fixtures, deterministic per-byte mutations, scalar
encodings, record nesting/slot identity, source pointer/byte preservation,
budget edges and rejection in registered native/WASM conversion routes.
These are synthetic structural tests, not fuzzing, accuracy or visual parity.

Final local checks: 359 light/410 native conversion tests (including doc-tests),
both Clippy configurations with warnings denied, WASM compilation, formatter,
workspace default tests and the 28-file/19-tree freeze check pass. Existing
environment-dependent ignored tests remain ignored; no vendor runtime or real
Office test was performed for this MTEF milestone. Remote CI must be checked
for the exact uploaded commit, not inferred from these results.

Remaining FMT-06 gates:

- [ ] Acquire permitted real v5/legacy/container samples with reproducible
  provenance; validate against independently exported structure, not this reader.
- [ ] Extend the finite reference/slot diagnostics above into a separately
  reviewed MTCode/reference map and finite AST conversion with loss reports;
  retain unsupported records and all original bytes.
- [ ] Review raw extraction and safe host read-back without activating or writing
  third-party OLE; prove source/asset preservation on copies and save/reopen.
- [ ] Evaluate SDK/interface/asset licensing independently before any vendor
  integration, writing or redistribution; do not infer approval from public docs.
- [ ] Only then consider registered import directions. Any write/MathType layout
  compatibility needs its own versioned design, fixtures and host acceptance.
