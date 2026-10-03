# Generated formula string conversion capabilities

Generated from `CapabilityRegistry::formula_conversions`; do not edit the table by hand.

Scope: Rust `DocumentConverter::convert_formula_string`, semantic string outputs only.
Compiled support is not complete syntax coverage, losslessness or visual parity.
Strict currently validates the supported LaTeX source subset before OMML export.
Best-effort may lose unsupported syntax/style; reconstructed LaTeX is not author source.

Python exposes `convert_formula` and `formula_conversion_capabilities`; C/JS mode-aware bindings remain pending.
The new entry point bounds source/reconstructed LaTeX to 64 KiB, 64 lexical nesting levels and 512 structural tokens.
These are conservative per-call budgets, not syntax validation; existing APIs are unchanged.
OLE is a host container, not a Core string format. VSTO is not a format.
Visual/package outputs still use the existing export registry and runtime requirements.

| Target | Input | Path | Best-effort outputs | Strict outputs |
|---|---|---|---|---|
| native | latex | source-first | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | omml |
| native | mathml | reconstructed-latex | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| native | omml | reconstructed-latex | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| native | typst | reconstructed-latex | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| native | markdown | document-ast | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| native | unicode-math | unsupported | unsupported | unsupported |
| native | ascii-math | unsupported | unsupported | unsupported |
| native | mtef | unsupported | unsupported | unsupported |
| wasm32-unknown-unknown | latex | source-first | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | omml |
| wasm32-unknown-unknown | mathml | reconstructed-latex | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| wasm32-unknown-unknown | omml | reconstructed-latex | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| wasm32-unknown-unknown | typst | reconstructed-latex | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| wasm32-unknown-unknown | markdown | document-ast | latex, latex_display, latex_equation, typst, markdown_inline, markdown_block, mathml, omml, html | unsupported |
| wasm32-unknown-unknown | unicode-math | unsupported | unsupported | unsupported |
| wasm32-unknown-unknown | ascii-math | unsupported | unsupported | unsupported |
| wasm32-unknown-unknown | mtef | unsupported | unsupported | unsupported |
