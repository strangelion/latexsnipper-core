//! Single-formula candidates, never document writes or implicit plugin loading.
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use latexsnipper_conversion::{
    conversion_registry::{
        ConversionFallbackOutcome, ConversionFallbackPolicy, ConversionRegistryErrorCode,
        RegisteredConversionRequest, SemanticConversionRegistry, CONVERSION_REGISTRY_VERSION,
    },
    FormulaConversionMode,
};
use sha2::{Digest, Sha256};

use crate::CliExitCode;

type Result<T> = std::result::Result<T, (CliExitCode, String)>;

#[derive(Subcommand)]
pub(crate) enum FormulaCommand {
    /// JSON capability projection; a profile backend is registered but disabled
    Capabilities {
        #[command(flatten)]
        extension: ExtensionOptions,
    },
    /// Emit an audited JSON candidate/refusal; stdout is never an Office write permit
    Convert {
        /// Canonical source format ID
        #[arg(long)]
        from: String,
        /// Canonical target format ID
        #[arg(long)]
        to: String,
        /// UTF-8 input path; '-' reads at most 64 KiB from stdin
        #[arg(long, default_value = "-")]
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = FormulaMode::BestEffort)]
        mode: FormulaMode,
        /// Explicit backend ID; omitted selects only core:builtin
        #[arg(long)]
        backend: Option<String>,
        /// Ordered availability-only alternatives; repeat at most three times
        #[arg(long)]
        fallback: Vec<String>,
        /// SHA-256 of definitions, fonts, styles and actual caller/host context
        #[arg(long)]
        context_sha256: String,
        /// Enable only the explicitly supplied profile route, not the package
        #[arg(long, requires = "profile")]
        enable_profile_backend: bool,
        #[command(flatten)]
        extension: ExtensionOptions,
    },
}

#[derive(Args)]
pub(crate) struct ExtensionOptions {
    /// Closed v1 JSON WASI route profile (requires the wasi-formula build feature)
    #[arg(long)]
    profile: Option<PathBuf>,
    /// Existing remote plugin store; does not install or enable any package
    #[arg(long, requires = "profile")]
    plugin_store: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum FormulaMode {
    Strict,
    BestEffort,
}

pub(crate) fn run(command: FormulaCommand) -> Result<()> {
    #[cfg(feature = "wasi-formula")]
    {
        let uses_profile = match &command {
            FormulaCommand::Capabilities { extension }
            | FormulaCommand::Convert { extension, .. } => extension.profile.is_some(),
        };
        if uses_profile {
            // Windows main-thread stacks are smaller than compiler/test worker
            // stacks. Reserve a fixed host stack for Wasmtime compilation; this
            // does not change guest memory, fuel or execution deadline grants.
            return std::thread::Builder::new()
                .name("snipper-wasi-formula".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(move || run_inner(command))
                .map_err(|error| {
                    (
                        CliExitCode::PluginFailure,
                        format!("cannot start WASI host worker: {error}"),
                    )
                })?
                .join()
                .map_err(|_| {
                    (
                        CliExitCode::PluginFailure,
                        "WASI formula host worker panicked; conversion was not retried".into(),
                    )
                })?;
        }
    }
    run_inner(command)
}

fn run_inner(command: FormulaCommand) -> Result<()> {
    match command {
        FormulaCommand::Capabilities { extension } => {
            let (registry, profile_sha256, _) = prepare(extension)?;
            print_json(&serde_json::json!({
                "schemaVersion": CONVERSION_REGISTRY_VERSION,
                "coreVersion": env!("CARGO_PKG_VERSION"),
                "wasiFormulaCompiled": cfg!(feature = "wasi-formula"),
                "profileSha256": profile_sha256,
                "formats": registry.extension_formats(),
                "routes": registry.capabilities(),
            }))
        }
        FormulaCommand::Convert {
            from,
            to,
            input,
            mode,
            backend,
            fallback,
            context_sha256,
            enable_profile_backend,
            extension,
        } => {
            if !is_sha256(&context_sha256) {
                return Err((
                    CliExitCode::InvalidArguments,
                    "context SHA-256 must be 64 lowercase hexadecimal characters".into(),
                ));
            }
            // Policy validation happens before package activation. Registry
            // conversion still owns the definitive policy/request validation.
            let policy = ConversionFallbackPolicy {
                schema_version: CONVERSION_REGISTRY_VERSION,
                backends: fallback,
            };
            policy
                .validate_for_backend(backend.as_deref())
                .map_err(|error| (CliExitCode::InvalidArguments, error.to_string()))?;
            let bytes = read_bounded(&input, 64 * 1024)?;
            let content = String::from_utf8(bytes).map_err(|_| {
                (
                    CliExitCode::InvalidInput,
                    "formula input is not UTF-8".into(),
                )
            })?;
            let (mut registry, profile_sha256, profile_backend) = prepare(extension)?;
            if enable_profile_backend {
                let id = profile_backend.ok_or_else(|| {
                    (
                        CliExitCode::InvalidArguments,
                        "profile backend was not prepared".into(),
                    )
                })?;
                registry
                    .set_backend_enabled(&id, true)
                    .map_err(|error| (CliExitCode::PluginFailure, error.to_string()))?;
            }
            let request = RegisteredConversionRequest {
                schema_version: CONVERSION_REGISTRY_VERSION,
                input: from,
                output: to,
                content,
                backend,
                mode: match mode {
                    FormulaMode::Strict => FormulaConversionMode::Strict,
                    FormulaMode::BestEffort => FormulaConversionMode::BestEffort,
                },
                context_sha256: effective_context(&context_sha256, profile_sha256.as_deref())?,
            };
            let report = registry.convert_with_fallback(&request, &policy);
            print_json(&report)?;
            match report.outcome {
                ConversionFallbackOutcome::Converted { .. } => Ok(()),
                ConversionFallbackOutcome::Rejected { error } => {
                    Err((exit_code(error.code), error.to_string()))
                }
            }
        }
    }
}

fn prepare(
    extension: ExtensionOptions,
) -> Result<(SemanticConversionRegistry, Option<String>, Option<String>)> {
    let Some(path) = extension.profile else {
        return Ok((SemanticConversionRegistry::default(), None, None));
    };
    if path == Path::new("-") {
        return Err((
            CliExitCode::InvalidArguments,
            "profile must be a file, not the formula stdin stream".into(),
        ));
    }
    #[cfg(not(feature = "wasi-formula"))]
    {
        let _ = (path, extension.plugin_store);
        Err((
            CliExitCode::PluginFailure,
            "WASI formula profiles require a build with --features wasi-formula".into(),
        ))
    }
    #[cfg(feature = "wasi-formula")]
    {
        let bytes = read_bounded(&path, 16 * 1024)?;
        let profile = latexsnipper_plugin_wasi::WasiFormulaProfile::from_json(&bytes)
            .map_err(|error| (CliExitCode::InvalidArguments, error.to_string()))?;
        let root = extension
            .plugin_store
            .unwrap_or_else(|| PathBuf::from(".snipper/plugins/signed-registries/remote"));
        let store = latexsnipper_plugin::RemotePluginStore::new(root);
        let prepared = profile
            .prepare(&store)
            .map_err(|error| (CliExitCode::PluginFailure, error.to_string()))?;
        Ok((
            prepared.registry,
            Some(prepared.profile_sha256),
            Some(prepared.backend_id),
        ))
    }
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let reader: Box<dyn Read> = if path == Path::new("-") {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(path).map_err(|error| {
            (
                CliExitCode::InvalidInput,
                format!("cannot open formula/profile input: {error}"),
            )
        })?)
    };
    let mut bytes = Vec::new();
    reader
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            (
                CliExitCode::InvalidInput,
                format!("cannot read formula/profile input: {error}"),
            )
        })?;
    if bytes.len() > maximum {
        return Err((
            CliExitCode::InvalidInput,
            format!("input exceeds {maximum} byte budget"),
        ));
    }
    Ok(bytes)
}

fn effective_context(caller: &str, profile: Option<&str>) -> Result<String> {
    let executable = std::env::current_exe()
        .and_then(std::fs::File::open)
        .map_err(|error| {
            (
                CliExitCode::GenericFailure,
                format!("cannot fingerprint actual Core executable: {error}"),
            )
        })?;
    let mut hasher = Sha256::new();
    let mut reader = std::io::BufReader::new(executable);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = reader.read(&mut buffer).map_err(|error| {
            (
                CliExitCode::GenericFailure,
                format!("cannot fingerprint actual Core executable: {error}"),
            )
        })?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
    }
    let build = format!("{:x}", hasher.finalize());
    let identity = serde_json::to_vec(&(
        "snipper-formula-context-v1",
        env!("CARGO_PKG_VERSION"),
        build,
        caller,
        profile,
    ))
    .map_err(|error| (CliExitCode::GenericFailure, error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(identity)))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let json = serde_json::to_vec(value)
        .map_err(|error| (CliExitCode::GenericFailure, error.to_string()))?;
    if json.len() > 512 * 1024 {
        return Err((
            CliExitCode::OutputValidationFailure,
            "JSON candidate report exceeds 512 KiB".into(),
        ));
    }
    use std::io::Write;
    let mut output = std::io::stdout().lock();
    output
        .write_all(&json)
        .and_then(|()| output.write_all(b"\n"))
        .map_err(|error| (CliExitCode::GenericFailure, error.to_string()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn exit_code(code: ConversionRegistryErrorCode) -> CliExitCode {
    use ConversionRegistryErrorCode::*;
    match code {
        InvalidDeclaration | VersionMismatch | DuplicateRegistration => {
            CliExitCode::InvalidArguments
        }
        InputRejected | LimitExceeded => CliExitCode::InvalidInput,
        OutputRejected => CliExitCode::OutputValidationFailure,
        BackendFailed => CliExitCode::PluginFailure,
        UnknownFormat | UnknownBackend | BackendDisabled | RouteUnavailable => {
            CliExitCode::UnsupportedConversion
        }
    }
}
