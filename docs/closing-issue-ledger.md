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
| O-06 | Batch insertion and clipboard ownership | Office integration maintainer | strangelion/LaTeXSnipper-Office | repeatable application harness with counts, timing and failure artifacts | P1 | not started |
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
| G-07 | Release WebView2 CSP, WASM and desktop startup | Desktop runtime maintainer | strangelion/LaTeXSnipper-Office | installed release smoke with console, CSP and runtime logs | P1 | not started |

## Release coordination

The release owner closes a row only when its required evidence is linked from a
generated report, checked-in fixture, or dated application test record.

The 2026-09-30 O-01 evidence, completed O-05 Word field recalculation evidence,
and partial O-02/O-03/O-04 real-host results are recorded in
`strangelion/LaTeXSnipper-Office` at `docs/office/real-host-acceptance.md`.
Office commit `0d0d34548bf8d0b9ab53b7a098b12dd604b04708` records 78/78 native
OMML save/reopen checks plus chapter `STYLEREF`/reset `SEQ`, `REF` and `PAGEREF`
displayed-value verification after reopen. O-02 through O-04 remain open until
their checked-in OOXML difference summaries and remaining host matrices are
complete.

- Core package evidence does not close OLE activation, clipboard, or real
  batch-insertion rows; O-05 is closed only by the linked real Word harness.
- A screenshot alone does not close an editability or round-trip row.
- A successful application smoke does not replace deterministic Core contracts.
- Current-release capability claims must omit or clearly label every open P0
  row.
- The tag gate also requires the release checklist, frozen-contract verification,
  clean tracked worktree, and green CI, WASM, CodeQL and scheduled hardening.
