use latexsnipper_foundation::{Result, SnipperError};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Model validation result.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Whether the model passed validation.
    pub valid: bool,
    /// SHA-256 checksum of the model file.
    pub checksum: String,
    /// File size in bytes.
    pub file_size: u64,
    /// Any warnings or issues found.
    pub warnings: Vec<String>,
}

impl ValidationReport {
    /// Create a successful validation report.
    pub fn success(checksum: String, file_size: u64) -> Self {
        Self {
            valid: true,
            checksum,
            file_size,
            warnings: Vec::new(),
        }
    }

    /// Create a failed validation report.
    pub fn failure(checksum: String, file_size: u64, warnings: Vec<String>) -> Self {
        Self {
            valid: false,
            checksum,
            file_size,
            warnings,
        }
    }
}

/// Compute SHA-256 checksum of a file.
pub fn compute_checksum(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| {
        SnipperError::Model(format!("Failed to read file for checksum: {}", e))
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

/// Compute SHA-256 checksum of a byte slice.
pub fn compute_bytes_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Validate a model file against an expected checksum.
pub fn validate_model(path: &Path, expected_checksum: &str) -> Result<ValidationReport> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        SnipperError::Model(format!("Failed to read model metadata: {}", e))
    })?;

    let file_size = metadata.len();
    let checksum = compute_checksum(path)?;

    let mut warnings = Vec::new();

    if checksum != expected_checksum {
        warnings.push(format!(
            "Checksum mismatch: expected {}, got {}",
            expected_checksum, checksum
        ));
    }

    if file_size == 0 {
        warnings.push("Model file is empty".into());
    }

    if file_size > 500_000_000 {
        warnings.push(format!(
            "Model file is unusually large: {} bytes",
            file_size
        ));
    }

    let valid = checksum == expected_checksum && !warnings.iter().any(|w| w.contains("empty"));

    Ok(ValidationReport {
        valid,
        checksum,
        file_size,
        warnings,
    })
}

/// Validate model bytes against an expected checksum.
pub fn validate_model_bytes(data: &[u8], expected_checksum: &str) -> ValidationReport {
    let checksum = compute_bytes_checksum(data);
    let file_size = data.len() as u64;

    let mut warnings = Vec::new();

    if checksum != expected_checksum {
        warnings.push(format!(
            "Checksum mismatch: expected {}, got {}",
            expected_checksum, checksum
        ));
    }

    if file_size == 0 {
        warnings.push("Model data is empty".into());
    }

    let valid = checksum == expected_checksum && file_size > 0;

    ValidationReport {
        valid,
        checksum,
        file_size,
        warnings,
    }
}

/// Load and validate checksums from a CHECKSUMS.sha256 file.
pub fn load_checksums(path: &Path) -> Result<std::collections::HashMap<String, String>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        SnipperError::Model(format!("Failed to read checksums file: {}", e))
    })?;

    let mut checksums = std::collections::HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((hash, filename)) = line.split_once(' ') {
            let filename = filename.trim().to_string();
            let hash = hash.trim().to_string();
            checksums.insert(filename, hash);
        }
    }

    Ok(checksums)
}

/// Validate all models in a directory against checksums.
pub fn validate_all_models(
    models_dir: &Path,
    checksums_path: &Path,
) -> Result<Vec<(String, ValidationReport)>> {
    let checksums = load_checksums(checksums_path)?;
    let mut results = Vec::new();

    for (filename, expected_hash) in &checksums {
        let model_path = models_dir.join(filename);
        if model_path.exists() {
            match validate_model(&model_path, expected_hash) {
                Ok(report) => results.push((filename.clone(), report)),
                Err(e) => results.push((
                    filename.clone(),
                    ValidationReport::failure(String::new(), 0, vec![e.to_string()]),
                )),
            }
        } else {
            results.push((
                filename.clone(),
                ValidationReport::failure(String::new(), 0, vec!["File not found".into()]),
            ));
        }
    }

    Ok(results)
}
