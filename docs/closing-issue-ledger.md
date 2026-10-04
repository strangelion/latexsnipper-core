# Closing issue ledger

This ledger turns the closing plan into ownership and evidence gates. It is a
release coordination document, not a statement that every listed capability is
already supported. Update its status and evidence links before creating a
release tag.

## Status and blocker vocabulary

Status:

- done: the named evidence exists and its automated gate passes;
- in progress: implementation exists, but the named acceptance evidence is
  incomplete;
- blocked: progress requires an external asset, license, application, or user
  decision;
- not started: no qualifying evidence has been recorded.

Blocker:

- P0 claim blocker: the product must not claim this capability without evidence;
- P1 release blocker: required for the planned release candidate;
- P2 UX blocker: release may proceed only if the known limitation is disclosed;
- P3 follow-up: useful work that is not part of the current release gate.

## Core

| ID | Item | Owner | Repository | Required evidence | Blocker | Status |
|---|---|---|---|---|---|---|
| C-01 | Deterministic 10,000 formula and 500 mixed-document corpora | Core evaluation maintainer | strangelion/latexsnipper-core | frozen manifests, digests, pilot and full reports | P1 | done |
| C-02 | Parser, conversion, round-trip, latency, throughput and peak-memory evidence | Core benchmark maintainer | strangelion/latexsnipper-core | locked tests and generated benchmark report | P1 | done |
| C-03 | Counted DOCX OMML, PNG and SVG multi-object round trip | Core fidelity maintainer | strangelion/latexsnipper-core | checksum-pinned fixture and package capability report | P1 | done |
| C-04 | Word bookmark, SEQ, REF and safe field-refresh request | Core conversion maintainer | strangelion/latexsnipper-core | DOCX read-back tests and package tokens; Word recalculation excluded | P0 claim blocker | done |
| C-05 | Accuracy claims on externally sourced, real-distribution formula data | Evaluation and release owners | strangelion/latexsnipper-core | licensed corpus provenance, model identity, accuracy report | P0 claim blocker | blocked |
| C-06 | Optional Zig interop decision | Core performance maintainer | strangelion/latexsnipper-core | isolated ABI benchmark and explicit adoption decision | P3 | done |

## Office and OLE

| ID | Item | Owner | Repository | Required evidence | Blocker | Status |
|---|---|---|---|---|---|---|
| O-01 | Package and install the x64 OLE server DLL | Office packaging maintainer | strangelion/LaTeXSnipper-Office | clean installer log, installed DLL hash, COM registration and activation result | P0 claim blocker | done |
| O-02 | Word formula, image and OLE insert-save-reopen-readback | Office integration maintainer | strangelion/LaTeXSnipper-Office | document fixtures, screenshots, OOXML diff and editable readback result | P0 claim blocker | in progress |
| O-03 | Excel formula, image and supported object insert-save-reopen-readback | Office integration maintainer | strangelion/LaTeXSnipper-Office | workbook fixtures, screenshots, OOXML diff and readback result | P0 claim blocker | in progress |
| O-04 | PowerPoint formula, image and supported object insert-save-reopen-readback | Office integration maintainer | strangelion/LaTeXSnipper-Office | presentation fixtures, screenshots, OOXML diff and readback result | P0 claim blocker | in progress |
| O-05 | Word recalculates dirty SEQ and REF fields | Office integration maintainer | strangelion/LaTeXSnipper-Office | Word automation or manual harness showing updated displayed values after reopen | P0 claim blocker | done |
| O-06 | Batch insertion and clipboard ownership | Office integration maintainer | strangelion/LaTeXSnipper-Office | repeatable application harness with counts, timing and failure artifacts | P1 | in progress |
| O-07 | Office command layout, sizing and typography | Office UX maintainer | strangelion/LaTeXSnipper-Office | wide and portrait screenshots plus interaction regression checklist | P2 | in progress |

## GUI and desktop runtime

| ID | Item | Owner | Repository | Required evidence | Blocker | Status |
|---|---|---|---|---|---|---|
| G-01 | Math editor and mixed custom-symbol preview | Formula editor maintainer | strangelion/LaTeXSnipper-Office | visual fixtures for plain, stacked, subscript and superscript formulas | P1 | in progress |
| G-02 | TikZ, PGFPlots, Graphviz and Mermaid safe preview | Drawing workspace maintainer | strangelion/LaTeXSnipper-Office | real WebView2 preview matrix, timeout diagnostics and output bounds checks | P1 | in progress |
| G-03 | Formula library and custom-symbol discoverability | Formula library maintainer | strangelion/LaTeXSnipper-Office | persisted-library reload and searchable thumbnail regression | P1 | in progress |
| G-04 | Source syntax colors, theme controls and light-blue default | Office UX maintainer | strangelion/LaTeXSnipper-Office | light, dark and custom-theme visual regression at supported scales | P2 | in progress |
| G-05 | Portrait layout, symbol rail, canvas toolbar and popup positioning | Office UX maintainer | strangelion/LaTeXSnipper-Office | portrait and narrow-window screenshots without avoidable scrollbars or clipping | P2 | in progress |
| G-06 | Tray and taskbar thumbnail left/right click behavior | Desktop runtime maintainer | strangelion/LaTeXSnipper-Office | packaged Tauri interaction checklist on Windows | P1 | in progress |
| G-07 | Release WebView2 CSP, WASM and desktop startup | Desktop runtime maintainer | strangelion/LaTeXSnipper-Office | installed release smoke with console, CSP and runtime logs | P1 | in progress |

## Release coordination

The release owner closes a row only when its required evidence is linked from a
generated report, checked-in fixture, or dated application test record.

The 2026-09-30 O-01 evidence, completed O-05 Word field recalculation evidence,
and partial O-02/O-03/O-04 real-host results are recorded in
`strangelion/LaTeXSnipper-Office` at `docs/office/real-host-acceptance.md`.
Office harness commit `4cf6d74d45396fbd5b7fc096294b138d7502b516` and evidence
record `9eea8707d5c992f58c20b9b813e613d9f6703b98` cover a stale-value
transition: after deleting the preceding numbered formula, target `SEQ` and
`REF` remain at `2`, a document-wide field update changes both to `1`, and the
values remain `1` after save/reopen. O-02 through O-04 remain open until their
checked-in OOXML difference summaries and remaining host matrices are complete.

- Core package evidence does not close OLE activation, clipboard, or real
  batch-insertion rows; O-05 is closed only by the linked real Word harness.
- O-06 partial evidence (2026-10-02): real Word scanned 250 instances of one
  Core-generated integral with four delimiter forms. 249 converted, one injected
  invalid payload preserved its source; adjacent prose and counts survived reopen.
  Clipboard sequence stayed unchanged. A 100-item chunk took 114.5 seconds, so
  Office now uses 25-item chunks. This is not formula-diversity accuracy, pipe
  timeout reconciliation, actual clipboard paste, or Excel/PowerPoint coverage.
- A screenshot alone does not close an editability or round-trip row.
- O-06 regression (2026-10-03): Office Word/Excel/PowerPoint batch handlers now
  return the original request/session IDs; previously empty IDs could leave the
  desktop waiting after the document was already modified. Shared C# wire tests
  and five Tokio waiter tests cover correlation, late completion, bounded final
  timeout and disconnect cleanup. These are not real-host end-to-end acceptance;
  the row remains in progress. See the dated Office real-host acceptance record.
- A successful application smoke does not replace deterministic Core contracts.
- O-06 story acceptance (2026-10-03): Office's real Word harness converted seven
  body/header/footer/text-frame formulas, retained adjacent prose and persistent
  IDs/source/OMML after reopen, rejected invalid locators and stale hashes, and
  tested duplicate/Unicode/long source positions. A floating shape anchor had
  shifted body offsets because Word positions differ from plain-text offsets;
  story-local lookup and exact source/prefix checks now guard that case.
  The 250-item body regression remains 249 converted, one injected invalid item
  preserved, zero execution failures, unchanged clipboard and successful reopen;
  328.093 seconds total, 18.136–48.795 seconds per 25-item chunk. Not a diversity
  accuracy or speedup claim. Pipe, display layout and other-host rows remain open.
- O-06 follow-up explicitly queued (2026-10-03): undelimited LaTeX selection
  conversion, opt-in full-document candidates, preprocessed ID/revision/hash
  indexing, unchanged-formula caching, batched manifest writes and field refresh,
  and an optional saved-copy-only offline DOCX path. Profile and benchmark before
  claiming speed improvements. See Office `docs/office/batch-update-plan.md`.
- Office package verification run 37098619175 at bb268a6 passed on all three
  platforms; the Windows unattended certificate step and install/reinstall/
  upgrade/uninstall passed. This validates that commit's package gate, not the
  newer story-fix source or release WebView2/real desktop pipe acceptance.
- Current-release capability claims must omit or clearly label every open P0
  row.
- O-06 raw-selection progress (2026-10-03): Office's real Word native harness
  read fractions, scripts, matrices and multiline selections without mutation,
  rejected prose/paths/incomplete syntax, retained rejected/stale duplicate
  sources, converted three body/header/text-frame selections and retained
  source/OMML after reopen (4.578 seconds). Desktop and Office.js preview/confirm
  UI is being verified separately; this native harness does not close their
  end-to-end pipe or Office.js-host gates. Core now exposes an opt-in strict
  OMML guard so unknown commands/environments cannot silently disappear during
  source replacement; 245 conversion unit tests and one doc-test pass without
  default features. Legacy best-effort export remains compatible.
- User follow-up (2026-10-03): add explicit source/target conversion choices,
  including LaTeXSnipper OLE versus native OMML. VSTO is an add-in technology,
  not a target format. MathType/MTEF and third-party OLE import/export need
  separate capability detection and fixtures; do not advertise generic OLE
  compatibility before proving it.
- The tag gate also requires the release checklist, frozen-contract verification,
  clean tracked worktree, and green CI, WASM, CodeQL and scheduled hardening.
- O-06 stage profiling (2026-10-04): Office's real Word harness measured 25 and
  250 repeated integral candidates. 24/249 converted, one intentionally invalid
  payload preserved in each run, zero execution failures; full manifest IDs and
  payloads matched after save/reopen and the clipboard sequence was unchanged.
  Runs took 17.247/232.841 seconds; the 250-item candidate total was 220.019 seconds,
  including 181.220 seconds of scratch materialization/copying. Nested stages
  overlap and cannot be summed. Core conversion, pipe, field refresh, 1,000-item
  and diverse 10,000-item matrices remain unmeasured. An earlier extra read-back
  rejection is still unexplained; current successful reruns do not close that
  intermittent risk. Evidence is recorded in Office's real-host acceptance file.
  O-06 remains in progress; no speedup or diversity accuracy claim is made.
- Format follow-up priority (2026-10-04): finish real OLE/image host closure first,
  then implement separate bounded UnicodeMath and AsciiMath syntax/AST/round-trip
  pilots, and a versioned read-only MTEF milestone before any third-party write.
  See `docs/formula-format-roadmap.md`; all three still require their own gates.
- FMT-05 partial implementation (2026-10-04): AsciiMath now has an independent
  experimental Rust lexer/parser and supported AST serializer, versioned synthetic
  fixtures and explicit size/token/depth limits. Twelve focused tests cover
  fraction/script binding, nested matrices, rejection and canonical structural
  round trips, including 4,913 deterministic three-token combinations. One
  MathML/OMML bridge fixture checks structure only. Expanded corpus,
  typography/host parity and registered API/UI directions remain open; the
  production capability registry still rejects AsciiMath rather than implying
  support from the existence of an experimental parser.
- FMT-05 next bounded milestone (2026-10-04): separate UnicodeMath lexer/parser,
  supported AST serializer, 16 accepted/8 rejected versioned fixtures and 13
  focused tests (including 600 deterministic combinations and one MathML/OMML
  matrix bridge). Operand/script/space and padded-matrix rules are independent
  of AsciiMath; only the private AST comparator is shared. Light/native conversion
  regressions, Clippy and WASM compilation pass. No registered API/UI, full Unicode
  property grammar, external accuracy or Word visual parity claim is made; FMT-05
  and real OLE/image host closure stay open. Previous Core 37ce6d8 CI 37185881099,
  WASM 37185881098 and CodeQL 37185881097 have been verified successful.
- FMT-06 partial milestone (2026-10-04): experimental raw MTEF v5 read-only
  inspection retains the full source and decoded record spans/depth. Fifteen
  focused synthetic tests cover framing, truncation, mutations, budgets and
  registered-conversion rejection. Complete framing is not semantic validity,
  mathematical accuracy or real MathType compatibility. No SDK, third-party
  assets, OLE activation, AST converter, writer or API/UI exposure was added;
  real sample provenance, licensing and host/container gates remain open.
  See `docs/formats/mtef-readonly.md`; FMT-06 is not closed.
- Optional follow-up (2026-10-04): personalized candidate ranking, formula
  completion and controlled continuation are now recorded separately in
  `docs/formula-assistance-roadmap.md`. This is a proposed evaluation only,
  not a model integration or approval to collect/upload user documents.
- Office partial evidence (2026-10-04): an isolated local release WebView2
  passed WASM/CSP and three drawing/custom-symbol checks; a real Tauri-to-Word
  development add-in pipe converted four inline equations in 2.318 seconds and
  survived save/reopen with adjacent prose. One earlier loaded cold run failed;
  its cause and late-result risks remain open. Installed NativeOffice provenance
  is older, so G-07 does not close. See Office real-host acceptance records.
- New Office requests are recorded in its batch-update plan: connected-document
  selection, enumeration of all open documents, more LaTeX insertion/conversion
  targets and direct host commands. The connected-session picker/browser tests
  are partial progress, not real multi-document or complete OLE/image acceptance.
