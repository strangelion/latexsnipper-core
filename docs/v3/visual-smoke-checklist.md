# Visual Smoke Test Checklist — Core 3.0.0 GA

**Purpose:** Verify that generated Office/PDF files open without corruption
in their respective applications and contain expected content.

**Note:** This is a limited smoke test, not a pixel-level fidelity audit.
Core 3.0.0 explicitly does not promise Microsoft Office visual parity.
See README.md for supported fidelity guarantees.

---

## Pre-requisites

1. Build the CLI:
   ```bash
   cargo build --release -p latexsnipper-cli
   ```

2. Ensure test fixtures are available:
   ```bash
   ls tests/fixtures/docx/test.docx
   ls tests/fixtures/pptx/test.pptx
   ls tests/fixtures/xlsx/test.xlsx
   ls fidelity/fixtures/office-rich.docx
   ls fidelity/fixtures/presentation-rich.pptx
   ls fidelity/fixtures/workbook-rich.xlsx
   ```

---

## Test Matrix

### Word (DOCX)

| # | Action | Expected Result | Pass |
|---|--------|-----------------|------|
| 1 | Open `tests/fixtures/docx/test.docx` | No corruption dialog | [ ] |
| 2 | Verify text content visible | Text paragraphs render correctly | [ ] |
| 3 | Verify formulas visible | Formulas appear (may be OMML source) | [ ] |
| 4 | Open `fidelity/fixtures/office-rich.docx` | No corruption dialog | [ ] |
| 5 | Verify tables visible | Tables render with borders | [ ] |
| 6 | Verify images visible | Images render (or alt text shown) | [ ] |

### PowerPoint (PPTX)

| # | Action | Expected Result | Pass |
|---|--------|-----------------|------|
| 1 | Open `tests/fixtures/pptx/test.pptx` | No corruption dialog | [ ] |
| 2 | Verify slide text visible | Text renders correctly | [ ] |
| 3 | Verify slide layout | Slides display in correct order | [ ] |
| 4 | Open `fidelity/fixtures/presentation-rich.pptx` | No corruption dialog | [ ] |
| 5 | Verify shapes visible | Shapes render correctly | [ ] |

### Excel (XLSX)

| # | Action | Expected Result | Pass |
|---|--------|-----------------|------|
| 1 | Open `tests/fixtures/xlsx/test.xlsx` | No corruption dialog | [ ] |
| 2 | Verify cell data visible | Cell values render correctly | [ ] |
| 3 | Verify formulas calculated | Formula cells show results | [ ] |
| 4 | Open `fidelity/fixtures/workbook-rich.xlsx` | No corruption dialog | [ ] |
| 5 | Verify sheet tabs visible | Multiple sheets accessible | [ ] |

### PDF

| # | Action | Expected Result | Pass |
|---|--------|-----------------|------|
| 1 | Generate PDF: `snipper convert input.docx -f pdf -o output.pdf` | PDF generated without error | [ ] |
| 2 | Open `output.pdf` in PDF viewer | No corruption dialog | [ ] |
| 3 | Verify text selectable | Text can be selected and copied | [ ] |
| 4 | Verify pages render | All pages visible | [ ] |

### SVG/PNG

| # | Action | Expected Result | Pass |
|---|--------|-----------------|------|
| 1 | Generate SVG: `snipper convert input.docx -f svg -o output.svg` | SVG generated | [ ] |
| 2 | Open `output.svg` in browser | SVG renders correctly | [ ] |
| 3 | Generate PNG: `snipper convert input.docx -f png -o output.png` | PNG generated | [ ] |
| 4 | Open `output.png` in image viewer | Image renders correctly | [ ] |

---

## Known Visual Differences

Document any visual differences from the source format here:

| Format | Difference | Severity | Acceptable for GA |
|--------|-----------|----------|-------------------|
| DOCX | Formulas may show as OMML source | Medium | Yes (documented) |
| PPTX | Complex layouts may reflow | Low | Yes (documented) |
| XLSX | Conditional formatting may not transfer | Low | Yes (documented) |
| PDF | Unicode characters outside WinAnsi degrade to `?` | Medium | Yes (documented) |
| SVG | Formulas render as visual text, not math layout | Medium | Yes (documented) |

---

## How to Run Automated Smoke Tests

```bash
# Run the full Office/PDF corpus tests
cargo test --locked -p latexsnipper-tests -- office
cargo test --locked -p latexsnipper-tests -- pdf

# Run the fidelity tests
cargo test --locked -p latexsnipper-fidelity

# Run the fixture tests
cargo test --locked -p latexsnipper-tests -- fixture
```

---

## Sign-off

After completing the manual smoke tests above, record:

- **Tester:** _______________
- **Date:** _______________
- **Platform:** _______________ (Windows/macOS/Linux + version)
- **Office version:** _______________ (if applicable)
- **PDF viewer:** _______________
- **Result:** PASS / FAIL (with notes)
