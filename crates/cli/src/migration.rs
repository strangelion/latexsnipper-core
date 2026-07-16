use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use latexsnipper_ast::{DiagnosticLevel, Document, NormalizeAssetOptions, DOCUMENT_SCHEMA_VERSION};
use latexsnipper_foundation::{MigrationReport, MigrationStatus, MigrationWarning};
use latexsnipper_model::{ModelManifest, ModelManifestV3, MODEL_MANIFEST_SCHEMA_VERSION_V3};
use latexsnipper_plugin::{PluginManifest, PluginManifestV3, PLUGIN_MANIFEST_SCHEMA_VERSION_V3};
use serde::Serialize;

use crate::fs_util;

const MAX_MIGRATION_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Subcommand)]
pub enum MigrationCommand {
    /// Migrate a legacy plugin manifest to manifest schema v3
    PluginManifest(MigrationArgs),
    /// Migrate a legacy model manifest to manifest schema v3
    ModelManifest(MigrationArgs),
    /// Normalize a serialized Document without changing its schema contract
    Document(MigrationArgs),
    /// Inspect a serialized contract and report migration requirements
    Inspect(MigrationInspectArgs),
}

#[derive(Debug, Args)]
pub struct MigrationArgs {
    /// Source JSON file
    pub input: PathBuf,
    /// Destination file; defaults to a new sibling file
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,
    /// Replace an existing destination; the source can never be replaced
    #[arg(long)]
    pub force: bool,
    /// Emit the migration report as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct MigrationInspectArgs {
    /// Source JSON file
    pub input: PathBuf,
    /// Emit the inspection report as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationRunStatus {
    Completed,
    RequiresManualAction,
}

#[derive(Debug)]
struct PreparedMigration {
    contract: &'static str,
    report: MigrationReport,
    value: Option<serde_json::Value>,
    default_suffix: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MigrationCliReport<'a> {
    operation: &'a str,
    source: String,
    output: Option<String>,
    wrote_output: bool,
    report: &'a MigrationReport,
}

pub fn run(command: MigrationCommand) -> Result<MigrationRunStatus, String> {
    match command {
        MigrationCommand::PluginManifest(args) => run_migration(args, prepare_plugin_manifest),
        MigrationCommand::ModelManifest(args) => run_migration(args, prepare_model_manifest),
        MigrationCommand::Document(args) => run_migration(args, prepare_document),
        MigrationCommand::Inspect(args) => inspect(args),
    }
}

fn run_migration(
    args: MigrationArgs,
    prepare: fn(&[u8]) -> Result<PreparedMigration, String>,
) -> Result<MigrationRunStatus, String> {
    let bytes = read_bounded(&args.input)?;
    let prepared = prepare(&bytes)?;

    if prepared.report.status == MigrationStatus::RequiresManualAction {
        print_report(
            prepared.contract,
            &args.input,
            None,
            false,
            &prepared.report,
            args.json,
        )?;
        return Ok(MigrationRunStatus::RequiresManualAction);
    }

    let output = args
        .output
        .unwrap_or_else(|| default_output(&args.input, prepared.default_suffix));
    ensure_distinct_paths(&args.input, &output)?;

    let value = prepared
        .value
        .ok_or_else(|| "migration produced no output value".to_string())?;
    let mut encoded = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("could not serialize migrated value: {error}"))?;
    encoded.push(b'\n');
    write_new_output(&output, &encoded, args.force)?;
    print_report(
        prepared.contract,
        &args.input,
        Some(&output),
        true,
        &prepared.report,
        args.json,
    )?;
    Ok(MigrationRunStatus::Completed)
}

fn inspect(args: MigrationInspectArgs) -> Result<MigrationRunStatus, String> {
    let bytes = read_bounded(&args.input)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("invalid JSON: {error}"))?;
    let prepared = if looks_like_plugin_manifest(&value) {
        prepare_plugin_manifest(&bytes)?
    } else if looks_like_model_manifest(&value) {
        prepare_model_manifest(&bytes)?
    } else if looks_like_document(&value) {
        prepare_document(&bytes)?
    } else {
        return Err(
            "could not identify plugin-manifest, model-manifest, or document contract".to_string(),
        );
    };
    let status = if prepared.report.status == MigrationStatus::RequiresManualAction {
        MigrationRunStatus::RequiresManualAction
    } else {
        MigrationRunStatus::Completed
    };
    print_report(
        prepared.contract,
        &args.input,
        None,
        false,
        &prepared.report,
        args.json,
    )?;
    Ok(status)
}

fn prepare_plugin_manifest(bytes: &[u8]) -> Result<PreparedMigration, String> {
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid JSON: {error}"))?;
    let schema = numeric_schema_version(&raw)?;
    if schema == Some(PLUGIN_MANIFEST_SCHEMA_VERSION_V3 as u64) {
        let value: PluginManifestV3 = serde_json::from_value(raw)
            .map_err(|error| format!("invalid plugin manifest v3: {error}"))?;
        value
            .validate_contract()
            .map_err(|error| format!("invalid plugin manifest v3 contract: {error}"))?;
        return Ok(PreparedMigration {
            contract: "plugin-manifest",
            report: MigrationReport::new(
                "plugin-manifest",
                "3",
                "plugin-manifest",
                "3",
                MigrationStatus::Unchanged,
            ),
            value: Some(
                serde_json::to_value(value)
                    .map_err(|error| format!("could not serialize plugin manifest: {error}"))?,
            ),
            default_suffix: "v3.json",
        });
    }
    if schema.is_some_and(|version| version > 2) {
        return Err(format!(
            "unsupported plugin manifest schema version {}",
            schema.unwrap_or_default()
        ));
    }

    let source: PluginManifest = serde_json::from_value(raw)
        .map_err(|error| format!("invalid legacy plugin manifest: {error}"))?;
    match PluginManifestV3::migrate_from_v2(source) {
        Ok(outcome) => Ok(PreparedMigration {
            contract: "plugin-manifest",
            report: outcome.report,
            value: Some(
                serde_json::to_value(outcome.value)
                    .map_err(|error| format!("could not serialize plugin manifest: {error}"))?,
            ),
            default_suffix: "v3.json",
        }),
        Err(error) => Ok(refused_migration(
            "plugin-manifest",
            "1",
            "3",
            "PLUGIN_V3_AUTOMATIC_MIGRATION_REFUSED",
            error.to_string(),
            "v3.json",
        )),
    }
}

fn prepare_model_manifest(bytes: &[u8]) -> Result<PreparedMigration, String> {
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid JSON: {error}"))?;
    let schema = numeric_schema_version(&raw)?;
    if schema == Some(MODEL_MANIFEST_SCHEMA_VERSION_V3 as u64) {
        let value: ModelManifestV3 = serde_json::from_value(raw)
            .map_err(|error| format!("invalid model manifest v3: {error}"))?;
        value
            .validate_contract()
            .map_err(|error| format!("invalid model manifest v3 contract: {error}"))?;
        return Ok(PreparedMigration {
            contract: "model-manifest",
            report: MigrationReport::new(
                "model-manifest",
                "3",
                "model-manifest",
                "3",
                MigrationStatus::Unchanged,
            ),
            value: Some(
                serde_json::to_value(value)
                    .map_err(|error| format!("could not serialize model manifest: {error}"))?,
            ),
            default_suffix: "v3.json",
        });
    }
    if schema.is_some_and(|version| version > 2) {
        return Err(format!(
            "unsupported model manifest schema version {}",
            schema.unwrap_or_default()
        ));
    }

    let source: ModelManifest = serde_json::from_value(raw)
        .map_err(|error| format!("invalid legacy model manifest: {error}"))?;
    match ModelManifestV3::migrate_from_v2(source) {
        Ok(outcome) => Ok(PreparedMigration {
            contract: "model-manifest",
            report: outcome.report,
            value: Some(
                serde_json::to_value(outcome.value)
                    .map_err(|error| format!("could not serialize model manifest: {error}"))?,
            ),
            default_suffix: "v3.json",
        }),
        Err(error) => Ok(refused_migration(
            "model-manifest",
            "2",
            "3",
            "MODEL_V3_AUTOMATIC_MIGRATION_REFUSED",
            error.to_string(),
            "v3.json",
        )),
    }
}

fn prepare_document(bytes: &[u8]) -> Result<PreparedMigration, String> {
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid JSON: {error}"))?;
    let declared = raw
        .get("schema_version")
        .or_else(|| raw.get("schemaVersion"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    if declared
        .as_deref()
        .is_some_and(|version| version != DOCUMENT_SCHEMA_VERSION)
    {
        return Err(format!(
            "unsupported document schema version {}",
            declared.as_deref().unwrap_or_default()
        ));
    }
    let mut document: Document =
        serde_json::from_value(raw).map_err(|error| format!("invalid document: {error}"))?;
    let mut diagnostics = document.migrate_inline_footnotes_to_notes();
    diagnostics.extend(document.normalize_assets(NormalizeAssetOptions {
        compute_checksum: false,
        infer_mime_type: true,
        deduplicate: false,
        fill_dimensions: false,
        migrate_legacy: true,
    }));
    document.schema_version = DOCUMENT_SCHEMA_VERSION.to_string();

    let status = if diagnostics.is_empty() && declared.is_some() {
        MigrationStatus::Unchanged
    } else {
        MigrationStatus::Migrated
    };
    let mut report = MigrationReport::new(
        "document",
        declared.as_deref().unwrap_or("legacy-unversioned"),
        "document",
        DOCUMENT_SCHEMA_VERSION,
        status,
    );
    for diagnostic in diagnostics {
        let warning = MigrationWarning::new(diagnostic.code, diagnostic.message);
        if diagnostic.level == DiagnosticLevel::Error || !diagnostic.recoverable {
            report.require_manual_action(warning);
        } else {
            report.push_warning(warning);
        }
    }
    Ok(PreparedMigration {
        contract: "document",
        report,
        value: Some(
            serde_json::to_value(document)
                .map_err(|error| format!("could not serialize document: {error}"))?,
        ),
        default_suffix: "migrated.json",
    })
}

fn refused_migration(
    contract: &'static str,
    source_version: &str,
    target_version: &str,
    code: &str,
    message: String,
    default_suffix: &'static str,
) -> PreparedMigration {
    let mut report = MigrationReport::new(
        contract,
        source_version,
        contract,
        target_version,
        MigrationStatus::RequiresManualAction,
    );
    report.require_manual_action(MigrationWarning::new(code, message));
    PreparedMigration {
        contract,
        report,
        value: None,
        default_suffix,
    }
}

fn numeric_schema_version(value: &serde_json::Value) -> Result<Option<u64>, String> {
    match value
        .get("schemaVersion")
        .or_else(|| value.get("schema_version"))
    {
        None => Ok(None),
        Some(version) => version
            .as_u64()
            .map(Some)
            .ok_or_else(|| "schema version must be an unsigned integer".to_string()),
    }
}

fn looks_like_plugin_manifest(value: &serde_json::Value) -> bool {
    value.get("pluginApiVersion").is_some()
        || value.get("executionClass").is_some()
        || (value.get("coreVersionRequirement").is_some() && value.get("id").is_some())
}

fn looks_like_model_manifest(value: &serde_json::Value) -> bool {
    value.get("sourceId").is_some() && value.get("categories").is_some()
}

fn looks_like_document(value: &serde_json::Value) -> bool {
    value.get("metadata").is_some() && value.get("pages").is_some()
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("source is not a file: {}", path.display()));
    }
    if metadata.len() > MAX_MIGRATION_INPUT_BYTES {
        return Err(format!(
            "source exceeds the {} byte migration limit",
            MAX_MIGRATION_INPUT_BYTES
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_MIGRATION_INPUT_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_MIGRATION_INPUT_BYTES {
        return Err(format!(
            "source exceeds the {} byte migration limit",
            MAX_MIGRATION_INPUT_BYTES
        ));
    }
    Ok(bytes)
}

fn default_output(source: &Path, suffix: &str) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("migrated");
    source.with_file_name(format!("{stem}.{suffix}"))
}

fn ensure_distinct_paths(source: &Path, output: &Path) -> Result<(), String> {
    let source = std::fs::canonicalize(source)
        .map_err(|error| format!("could not resolve source path: {error}"))?;
    let output = if output.exists() {
        std::fs::canonicalize(output)
            .map_err(|error| format!("could not resolve output path: {error}"))?
    } else {
        let parent = output
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = std::fs::canonicalize(parent)
            .map_err(|error| format!("could not resolve output directory: {error}"))?;
        let name = output
            .file_name()
            .ok_or_else(|| "output path has no file name".to_string())?;
        parent.join(name)
    };
    if source == output {
        return Err("migration output must not replace the source".to_string());
    }
    Ok(())
}

fn write_new_output(path: &Path, bytes: &[u8], force: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    if path.exists() && !force {
        return Err(format!(
            "output already exists: {}; pass --force to replace it",
            path.display()
        ));
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "invalid output file name".to_string())?;
    let temporary = parent.join(format!(
        ".{name}.snipper-migrate-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("could not create migration temporary file: {error}"))?;
        file.write_all(bytes)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("could not persist migration output: {error}"))?;
        drop(file);
        fs_util::activate_file(&temporary, path, force)
            .map_err(|error| format!("could not activate migration output: {error}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn print_report(
    operation: &str,
    source: &Path,
    output: Option<&Path>,
    wrote_output: bool,
    report: &MigrationReport,
    json: bool,
) -> Result<(), String> {
    if json {
        let value = MigrationCliReport {
            operation,
            source: source.display().to_string(),
            output: output.map(|path| path.display().to_string()),
            wrote_output,
            report,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&value)
                .map_err(|error| format!("could not serialize migration report: {error}"))?
        );
        return Ok(());
    }

    println!(
        "{} {} -> {}: {:?}",
        report.source_contract, report.source_version, report.target_version, report.status
    );
    for warning in &report.warnings {
        if let Some(field) = &warning.field {
            println!("  {} [{}]: {}", warning.code, field, warning.message);
        } else {
            println!("  {}: {}", warning.code, warning.message);
        }
    }
    if let Some(output) = output {
        println!("Wrote {}", output.display());
    } else if report.status == MigrationStatus::RequiresManualAction {
        println!("No output was written because manual action is required.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_future_schema_is_not_reinterpreted_as_legacy() {
        let error = prepare_plugin_manifest(br#"{"schemaVersion":4}"#).unwrap_err();
        assert!(error.contains("unsupported plugin manifest schema version 4"));
    }

    #[test]
    fn default_output_never_equals_source() {
        let source = Path::new("plugin.json");
        assert_eq!(
            default_output(source, "v3.json"),
            Path::new("plugin.v3.json")
        );
    }
}
