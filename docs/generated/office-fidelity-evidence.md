# Generated Office package fidelity evidence

This file is generated from the checksum-pinned fixtures in `fidelity/corpora/index.json`. Do not edit it by hand.

The measurements below execute Core import, same-format export, package validation, and read-back. They do **not** run Microsoft Word, Excel, or PowerPoint. Skipped application and visual layers remain unverified.

| Case | Format pair | structuralValidity | semanticPreservation | layoutPreservation | visualFidelity | editability | roundTripFidelity |
|---|---|---|---|---|---|---|---|
| docx-office-rich-v1 | DOCX -> DOCX | verified (1.000) | partial (0.888) | partial (1.000) | not-measured | partial | partial (0.888) |
| pptx-presentation-rich-v1 | PPTX -> PPTX | verified (1.000) | verified (1.000) | partial (1.000) | not-measured | partial | partial (1.000) |
| xlsx-workbook-rich-v1 | XLSX -> XLSX | verified (1.000) | partial (0.750) | partial (1.000) | not-measured | partial | partial (0.750) |
| pdf-rich-v1 | PDF -> PDF | verified (1.000) | partial (0.929) | partial (0.985) | not-measured | unsupported | unsupported |

## Declared package capability expectations

### docx-office-rich-v1

Status: `passed` — all declared package capability expectations matched

- `native-omml=preserved; minimum-occurrences=1; tokens=<m:oMath`
- `png-image=preserved; minimum-occurrences=1; tokens=word/media/image1.png`
- `svg-image=preserved; minimum-occurrences=1; tokens=word/media/image2.svg`
- `batch-native-omml=preserved; minimum-occurrences=4; tokens=<m:oMath>`
- `batch-png-assets=preserved; minimum-occurrences=4; tokens=.png"/>`
- `batch-svg-assets=preserved; minimum-occurrences=4; tokens=.svg"/>`
- `bookmark=preserved; minimum-occurrences=1; tokens=<w:bookmarkStart|w:name="eq_energy"`
- `sequence-field=preserved; minimum-occurrences=1; tokens=w:instr=" SEQ Equation \* ARABIC "`
- `cross-reference-field=preserved; minimum-occurrences=1; tokens=w:instr=" REF eq_energy \h "`
- `ole-editability=not-measured; limitation=the DOCX fixture does not install or activate an OLE server`
- `clipboard-paste=not-measured; limitation=clipboard ownership and paste require an application harness`
- `batch-insert=not-measured; limitation=batch insertion requires the Office adapter and application harness`

### pptx-presentation-rich-v1

Status: `passed` — all declared package capability expectations matched

- `png-image=preserved; minimum-occurrences=1; tokens=ppt/media/image1.png`
- `clipboard-paste=not-measured; limitation=clipboard ownership and paste require an application harness`
- `batch-insert=not-measured; limitation=batch insertion requires the Office adapter and application harness`

### xlsx-workbook-rich-v1

Status: `passed` — all declared package capability expectations matched

- `ole-editability=unsupported; diagnostics=W_OLE_NOT_SUPPORTED`
- `clipboard-paste=not-measured; limitation=clipboard ownership and paste require an application harness`
- `batch-insert=not-measured; limitation=batch insertion requires the Office adapter and application harness`

### pdf-rich-v1

Status: `skipped` — no package capability expectations are declared

- No package capability expectations declared.

## Evidence boundary

- `preserved` means the exported package still contains the declared deterministic token and reopens through Core.
- `unsupported` means Core emitted the required stable diagnostic; it is not a successful Office feature claim.
- `not-measured` means the capability requires the Office adapter or a real application harness.
- Visual parity, field recalculation, clipboard ownership, OLE activation, and batch insertion remain outside this package-only report.
