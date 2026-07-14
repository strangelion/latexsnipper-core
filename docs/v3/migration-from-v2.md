# Migrating from Core 2 to the Core 3 alpha contracts

PR 1 provides migration building blocks, not a complete v3 runtime. Keep using
the v2 WASM, CLI, plugin host, and model loader until the corresponding stacked
PR documents a tested replacement.

## Rust: inspect every migration outcome

```rust
use latexsnipper_foundation::MigrationStatus;
use latexsnipper_plugin::{PluginManifest, PluginManifestV3};

fn migrate_plugin(json: &str) -> Result<PluginManifestV3, Box<dyn std::error::Error>> {
    let old: PluginManifest = serde_json::from_str(json)?;
    let migrated = PluginManifestV3::migrate_from_v2(old)?;

    if migrated.report.status == MigrationStatus::RequiresManualAction {
        for warning in &migrated.report.warnings {
            eprintln!("{}: {}", warning.code, warning.message);
        }
        return Err("plugin manifest requires review".into());
    }

    migrated.value.validate_contract()?;
    Ok(migrated.value)
}
```

Never convert `native_abi` to trusted in-process execution. Never infer a WASI
WIT version, network scheme/port, or signature key identity from a v2 field.

```rust
use latexsnipper_foundation::MigrationStatus;
use latexsnipper_model::{ModelManifest, ModelManifestV3};

fn migrate_models(text: &str) -> Result<ModelManifestV3, Box<dyn std::error::Error>> {
    let old = ModelManifest::parse(text)?;
    let migrated = ModelManifestV3::migrate_from_v2(old)?;

    if migrated.report.status == MigrationStatus::RequiresManualAction {
        return Err("add profile metadata and executable validation evidence".into());
    }

    migrated.value.validate_contract()?;
    Ok(migrated.value)
}
```

All referenced model files and packages need exact SHA-256 values before
migration. A mechanically migrated profile is deliberately `unavailable`.

## Rust: v3 envelope construction

```rust
use latexsnipper_api_types::ApiEnvelopeV3;

let envelope = ApiEnvelopeV3::success("result", Vec::new());
assert!(envelope.has_valid_shape());
assert_eq!(envelope.versions.document_schema_version, "1.0.0");
```

This is a Rust contract example only. No v3 WASM export consumes or returns the
type in PR 1.

## TypeScript: keep version guards explicit

The shipped TypeScript client still speaks Worker protocol 1 and calls WASM API
v2. Do not change production callers to expect API v3 yet. A future adapter
should gate contracts independently:

```ts
type ContractVersionsV3 = {
  apiEnvelopeVersion: 3;
  capabilitySchemaVersion: 3;
  diagnosticSchemaVersion: 1;
  documentSchemaVersion: "1.0.0";
  coreVersion: string;
};

function acceptsContract(v: ContractVersionsV3): boolean {
  return v.apiEnvelopeVersion === 3 && v.documentSchemaVersion === "1.0.0";
}
```

This example is not part of the published declaration file yet. The later API
integration PR must add generated declarations, fixtures, and browser tests.

## CLI migration

There is no `snipper migrate-v3` command in PR 1, so no command is silently
advertised. Continue to use existing v2 CLI commands. Until a reviewed command
is added, migration tools should deserialize with the Rust helpers, write to a
new output file, print structured warnings, and refuse to overwrite the source
when `RequiresManualAction` is returned.

## Compatibility checklist

- Pin the exact alpha version; alpha contracts may change with documented notes.
- Preserve `Document.schemaVersion = "1.0.0"`.
- Treat every migration warning as actionable.
- Verify external plugin and model artifact SHA-256 values independently.
- Do not enable a migrated model profile until runtime and evidence metadata are
  complete.
- Do not claim WASI isolation, signed-registry trust, or v3 endpoint support
  until later PRs provide executable validation.
