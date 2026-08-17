# `.lsmodel` transport v1

Core owns the portable `.lsmodel` archive contract independently from the
model-manifest schema and crate version. Transport version 1 is a ZIP archive
with this mandatory layout:

```text
manifest.toml
model.onnx
tokenizer.json
... other manifest-declared artifacts
```

`manifest.toml` must be a regular file at the ZIP root. A wrapper layout such
as `my-model/manifest.toml` is invalid. Producers package the contents of the
model directory, not the directory itself.

The public `latexsnipper-runtime` helpers are:

- `inspect_lsmodel_archive(reader)` for bounded, non-extracting inspection;
- `create_lsmodel_archive(source_directory, output_path)` for deterministic
  root-layout creation;
- `create_lsmodel_archive_with_manifest(source_directory, output_path,
  manifest)` for migrating verified legacy model directories without changing
  their source files;
- `LSMODEL_TRANSPORT_VERSION`, `LSMODEL_MANIFEST_PATH`, and
  `LSMODEL_EXTENSION` for consumer capability reporting.

Inspection rejects unsafe paths, backslash paths, duplicate normalized names,
symbolic-link entries, missing declared artifacts, unsafe declared artifact
paths, SHA-256 mismatches, more than 4096 entries, manifests larger than 1 MiB,
and total declared uncompressed content larger than 64 GiB. Missing-root
diagnostics list the actual root entries and any nested manifest paths so a UI
can identify wrapper-directory packaging without extracting the archive.

The TOML manifest is parsed and validated by Core's runtime `ModelManifest`.
Applications must not invent a second manifest shape. A future transport
layout change increments `LSMODEL_TRANSPORT_VERSION`; a model-manifest change
increments its own schema/version contract separately.

Release maintainers migrate a catalog entry with:

```text
snipper models package --source <model-dir> --output <name>.lsmodel \
  --catalog scripts/model-manifest.template.json \
  --category <category> --variant <variant> --model-version <version>
snipper models inspect <name>.lsmodel --json
```

The manually dispatched `Model Release (.lsmodel v1)` workflow downloads the
previous model Release, verifies its published checksums, migrates every catalog
variant with those Core commands, and uploads the validated `.lsmodel` assets
directly to the target GitHub Release. `model-manifest.json`, `SHA256SUMS`, and
`release-provenance.json` bind that asset set to the exact Core commit.
