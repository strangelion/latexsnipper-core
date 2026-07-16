# Office and PDF fidelity guarantees

Core 3 measures six independent dimensions: package validity, semantic
preservation, layout preservation, visual fidelity, editability, and round-trip
fidelity. Passing one dimension never implies another.

## Supported guarantee

The checked-in DOCX, PPTX, XLSX, and PDF corpora guarantee that the repository
fixtures can be imported/exported without violating the declared structural and
semantic thresholds. Expected losses must produce stable diagnostics. Supported
assets and opaque package parts are checked independently.

This is not a claim of pixel-identical Microsoft Office, LibreOffice, browser,
or PDF-viewer output. Fonts, field evaluation, application-specific objects,
layout engines, print settings, animations, macros, and unsupported drawing
features can change appearance or editability.

## Executable evidence

```bash
cargo run -p latexsnipper-fidelity --bin fidelity-check -- \
  validate --index fidelity/corpora/index.json --repository-root .

cargo run -p latexsnipper-fidelity --bin fidelity-check -- \
  run --index fidelity/corpora/index.json --repository-root . \
  --source-commit <commit> --generated-at-utc <timestamp> \
  --output target/fidelity/report.json
```

The generated capability table is
[../generated/fidelity-capabilities.md](../generated/fidelity-capabilities.md).
Platform-specific visual approval requires opening the generated artifacts in
the named application/version and attaching screenshots or render outputs to the
release evidence. Until that approval exists, visual parity remains explicitly
best effort.
