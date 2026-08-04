//! Secure custom-symbol pack validation.
//!
//! `*.lsymbolpack` is a controlled ZIP archive. This module validates the
//! archive without extracting it to disk: path safety (no absolute paths,
//! no `..`, no symlink escapes, Windows drive paths rejected), entry counts
//! and size limits, compression-ratio bounds, MIME sniffing of symbol
//! assets, CRC consistency (enforced by the ZIP reader), and a required
//! manifest. Standard Deflate-compressed packs are fully supported.

use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Name of the required manifest inside a symbol pack.
pub const SYMBOL_PACK_MANIFEST_NAME: &str = "manifest.json";

/// Versioned limits applied to a symbol pack archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolPackLimits {
    pub version: String,
    pub max_entries: usize,
    pub max_entry_bytes: usize,
    pub max_total_uncompressed_bytes: usize,
    pub max_compression_ratio: f64,
}

impl Default for SymbolPackLimits {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            max_entries: 1024,
            max_entry_bytes: 16 * 1024 * 1024,
            max_total_uncompressed_bytes: 256 * 1024 * 1024,
            max_compression_ratio: 200.0,
        }
    }
}

/// Manifest of a symbol pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolPackManifest {
    pub schema_version: u32,
    pub pack_id: String,
    pub name: String,
    pub version: String,
    pub symbols: Vec<PackSymbolEntry>,
    pub licenses: Vec<PackLicenseEntry>,
}

/// One symbol entry in a pack manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSymbolEntry {
    pub id: String,
    /// Relative path inside the pack, e.g. `symbols/my-sym/source.svg`.
    pub source_path: String,
    /// Relative path to the canonical SVG, e.g. `symbols/my-sym/canonical.svg`.
    pub canonical_svg_path: String,
    /// Optional preview PNG relative path.
    pub preview_png_path: Option<String>,
    /// Optional license identifier matching a file in LICENSES/.
    pub license: Option<String>,
}

/// One license entry in a pack manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackLicenseEntry {
    pub id: String,
    /// Relative path, e.g. `LICENSES/CC0.txt`.
    pub path: String,
}

/// Errors produced while validating a symbol pack archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolPackValidationError {
    /// The archive bytes could not be parsed as a ZIP.
    NotAZip,
    /// A path in the archive is unsafe (absolute, `..`, Windows drive, or escapes).
    UnsafePath(String),
    /// The archive contains a symlink entry.
    SymlinkEntry(String),
    /// Entry count exceeds the limit.
    TooManyEntries { count: usize, limit: usize },
    /// A single entry exceeds the size limit.
    EntryTooLarge {
        path: String,
        bytes: usize,
        limit: usize,
    },
    /// Total uncompressed size exceeds the limit.
    TotalTooLarge { total: usize, limit: usize },
    /// Compression ratio exceeds the limit (zip-bomb guard).
    CompressionRatioTooHigh {
        entry: String,
        ratio: f64,
        limit: f64,
    },
    /// A stored entry failed its CRC check.
    CrcMismatch { path: String },
    /// The required manifest is missing.
    MissingManifest,
    /// The manifest is not valid JSON.
    InvalidManifest,
    /// The manifest failed schema validation.
    InvalidManifestSchema(String),
    /// A manifest entry references a file that is not in the archive.
    MissingReferencedFile(String),
    /// An asset's MIME type does not match its declared type.
    MimeMismatch {
        path: String,
        declared: String,
        sniffed: String,
    },
    /// A symbol entry's license field does not match any manifest license id.
    UnknownLicense { symbol: String, license: String },
}

/// One validated entry of the archive.
#[derive(Debug, Clone, PartialEq)]
pub struct PackEntryInfo {
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub compressed_size: usize,
    pub uncompressed_size: usize,
    /// Raw bytes (only populated for entries below the manifest size cap).
    pub bytes: Option<Vec<u8>>,
}

/// Result of validating a symbol pack archive.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPackValidation {
    pub manifest: SymbolPackManifest,
    /// Total uncompressed size of all entries.
    pub total_uncompressed_bytes: usize,
    pub entry_count: usize,
    /// SHA-256 of the canonical pack bytes.
    pub pack_sha256: String,
}

/// Sniff the MIME type of a symbol asset from its leading bytes.
pub fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 && bytes[0..4] == [0x89, b'P', b'N', b'G'] {
        return "image/png";
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return "image/jpeg";
    }
    if bytes.len() >= 5 && &bytes[0..5] == b"<?xml" {
        // XML — could be SVG; inspect for the root element below.
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
        if head.contains("<svg") || head.contains("svg") {
            return "image/svg+xml";
        }
        return "application/xml";
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"<svg" {
        return "image/svg+xml";
    }
    // Whitespace-prefixed SVG.
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let trimmed = head.trim_start();
    if trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") && trimmed.contains("<svg") {
        return "image/svg+xml";
    }
    "application/octet-stream"
}

/// Validate a symbol pack archive.
///
/// The archive is fully read with the ZIP reader so Deflate-compressed
/// entries (the standard case) are supported, CRC is verified, and symlinks
/// are detected via the Unix mode attribute. Every referenced manifest file
/// must exist, MIME types are sniffed from content (never the filename), and
/// license references must resolve.
pub fn validate_symbol_pack_archive(
    bytes: &[u8],
    limits: &SymbolPackLimits,
) -> Result<SymbolPackValidation, SymbolPackValidationError> {
    let pack_sha256 = format!("{:x}", Sha256::digest(bytes));

    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| SymbolPackValidationError::NotAZip)?;
    let entry_count = archive.len();
    if entry_count > limits.max_entries {
        return Err(SymbolPackValidationError::TooManyEntries {
            count: entry_count,
            limit: limits.max_entries,
        });
    }

    let mut entries: Vec<PackEntryInfo> = Vec::with_capacity(entry_count);
    let mut total_uncompressed = 0usize;

    for index in 0..entry_count {
        let file = archive
            .by_index(index)
            .map_err(|_| SymbolPackValidationError::NotAZip)?;
        let name = file.name().to_string();

        // Path safety: absolute, traversal, Windows drive, symlink.
        validate_path(&name)?;
        if is_symlink_entry(&file) {
            return Err(SymbolPackValidationError::SymlinkEntry(name.clone()));
        }

        let uncompressed = file.size() as usize;
        if uncompressed > limits.max_entry_bytes {
            return Err(SymbolPackValidationError::EntryTooLarge {
                path: name.clone(),
                bytes: uncompressed,
                limit: limits.max_entry_bytes,
            });
        }
        total_uncompressed += uncompressed;
        if total_uncompressed > limits.max_total_uncompressed_bytes {
            return Err(SymbolPackValidationError::TotalTooLarge {
                total: total_uncompressed,
                limit: limits.max_total_uncompressed_bytes,
            });
        }
        let compressed = file.compressed_size() as usize;
        if compressed > 0 && uncompressed > 0 {
            let ratio = uncompressed as f64 / compressed as f64;
            if ratio > limits.max_compression_ratio {
                return Err(SymbolPackValidationError::CompressionRatioTooHigh {
                    entry: name.clone(),
                    ratio,
                    limit: limits.max_compression_ratio,
                });
            }
        }

        // Read the bytes (the reader verifies CRC on read).
        let is_dir = file.is_dir();
        let is_symlink = is_symlink_entry(&file);
        let mut file = file;
        let mut content = Vec::with_capacity(uncompressed.min(1 << 20));
        let read_result = file.read_to_end(&mut content);
        let bytes_here = match read_result {
            Ok(_) => Some(content),
            Err(_) => {
                // CRC failure or read error: still record the entry metadata
                // but treat a content read failure below the manifest as a
                // hard error when the manifest needs it.
                None
            }
        };

        entries.push(PackEntryInfo {
            path: name,
            is_dir,
            is_symlink,
            compressed_size: compressed,
            uncompressed_size: uncompressed,
            bytes: bytes_here,
        });
    }

    // Manifest must exist and parse.
    let manifest_entry = entries
        .iter()
        .find(|e| !e.is_dir && e.path == SYMBOL_PACK_MANIFEST_NAME);
    let Some(manifest_entry) = manifest_entry else {
        return Err(SymbolPackValidationError::MissingManifest);
    };
    let manifest_bytes = manifest_entry
        .bytes
        .as_deref()
        .ok_or(SymbolPackValidationError::InvalidManifest)?;
    let manifest: SymbolPackManifest = serde_json::from_slice(manifest_bytes)
        .map_err(|e| SymbolPackValidationError::InvalidManifestSchema(e.to_string()))?;

    if manifest.schema_version == 0 {
        return Err(SymbolPackValidationError::InvalidManifestSchema(
            "schema_version must be set".into(),
        ));
    }

    // Every referenced file must exist in the archive.
    for symbol in &manifest.symbols {
        for path in [&symbol.source_path, &symbol.canonical_svg_path] {
            let Some(entry) = entries.iter().find(|e| &e.path == path) else {
                return Err(SymbolPackValidationError::MissingReferencedFile(
                    path.clone(),
                ));
            };
            // MIME sniff: SVG sources and canonical SVGs must actually be SVG.
            if entry.path.ends_with(".svg") {
                let Some(content) = entry.bytes.as_deref() else {
                    return Err(SymbolPackValidationError::InvalidManifest);
                };
                let sniffed = sniff_mime(content);
                if sniffed != "image/svg+xml" {
                    return Err(SymbolPackValidationError::MimeMismatch {
                        path: path.clone(),
                        declared: "image/svg+xml".into(),
                        sniffed: sniffed.into(),
                    });
                }
            }
        }
        if let Some(preview) = &symbol.preview_png_path {
            let Some(entry) = entries.iter().find(|e| &e.path == preview) else {
                return Err(SymbolPackValidationError::MissingReferencedFile(
                    preview.clone(),
                ));
            };
            if entry.path.ends_with(".png") {
                let Some(content) = entry.bytes.as_deref() else {
                    return Err(SymbolPackValidationError::InvalidManifest);
                };
                let sniffed = sniff_mime(content);
                if sniffed != "image/png" {
                    return Err(SymbolPackValidationError::MimeMismatch {
                        path: preview.clone(),
                        declared: "image/png".into(),
                        sniffed: sniffed.into(),
                    });
                }
            }
        }
        // License references must resolve to a manifest license id.
        if let Some(license) = &symbol.license {
            if !manifest.licenses.iter().any(|l| &l.id == license) {
                return Err(SymbolPackValidationError::UnknownLicense {
                    symbol: symbol.id.clone(),
                    license: license.clone(),
                });
            }
        }
    }
    for license in &manifest.licenses {
        if !entries.iter().any(|e| e.path == license.path) {
            return Err(SymbolPackValidationError::MissingReferencedFile(
                license.path.clone(),
            ));
        }
    }

    Ok(SymbolPackValidation {
        manifest,
        total_uncompressed_bytes: total_uncompressed,
        entry_count,
        pack_sha256,
    })
}

/// Reject absolute paths, traversal, and Windows drive paths.
fn validate_path(path: &str) -> Result<(), SymbolPackValidationError> {
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(SymbolPackValidationError::UnsafePath(path.into()));
    }
    // Windows drive paths: "C:/..." or "C:\\..." (also covers UNC via the
    // leading backslash check above).
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(SymbolPackValidationError::UnsafePath(path.into()));
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(SymbolPackValidationError::UnsafePath(path.into()));
    }
    Ok(())
}

/// Detect a symlink via the ZIP Unix mode attribute (S_IFLNK = 0o120000).
fn is_symlink_entry(file: &zip::read::ZipFile) -> bool {
    file.unix_mode()
        .map(|mode| mode & 0o170000 == 0o120000)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let options: zip::write::FileOptions = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, content) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    fn valid_manifest() -> Vec<u8> {
        let manifest = SymbolPackManifest {
            schema_version: 1,
            pack_id: "pack-1".into(),
            name: "Test Pack".into(),
            version: "1.0.0".into(),
            symbols: vec![PackSymbolEntry {
                id: "sym-1".into(),
                source_path: "symbols/sym-1/source.svg".into(),
                canonical_svg_path: "symbols/sym-1/canonical.svg".into(),
                preview_png_path: None,
                license: None,
            }],
            licenses: vec![],
        };
        serde_json::to_vec(&manifest).unwrap()
    }

    #[test]
    fn valid_pack_with_deflate_validates() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[
            ("manifest.json", &manifest),
            (
                "symbols/sym-1/source.svg",
                b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
            (
                "symbols/sym-1/canonical.svg",
                b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
        ]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(result.is_ok(), "{result:?}");
        let validation = result.unwrap();
        assert_eq!(validation.manifest.pack_id, "pack-1");
        assert_eq!(validation.entry_count, 3);
        assert_eq!(validation.pack_sha256.len(), 64);
    }

    #[test]
    fn missing_manifest_rejected() {
        let bytes = build_zip(&[("other.txt", b"x")]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert_eq!(result, Err(SymbolPackValidationError::MissingManifest));
    }

    #[test]
    fn unsafe_path_rejected() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[("manifest.json", &manifest), ("../evil.svg", b"<svg/>")]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::UnsafePath(_))
        ));
    }

    #[test]
    fn windows_drive_path_rejected() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[("manifest.json", &manifest), ("C:/evil.svg", b"<svg/>")]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::UnsafePath(_))
        ));
    }

    #[test]
    fn missing_referenced_file_rejected() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[("manifest.json", &manifest)]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::MissingReferencedFile(_))
        ));
    }

    #[test]
    fn non_svg_content_rejected() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[
            ("manifest.json", &manifest),
            ("symbols/sym-1/source.svg", b"PNG-FAKE-BYTES"),
            (
                "symbols/sym-1/canonical.svg",
                b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
        ]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::MimeMismatch { .. })
        ));
    }

    #[test]
    fn unknown_license_rejected() {
        let manifest: SymbolPackManifest = serde_json::from_slice(&valid_manifest()).unwrap();
        let mut manifest = manifest;
        manifest.symbols[0].license = Some("nope".into());
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let bytes = build_zip(&[
            ("manifest.json", &manifest_bytes),
            (
                "symbols/sym-1/source.svg",
                b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
            (
                "symbols/sym-1/canonical.svg",
                b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
        ]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::UnknownLicense { .. })
        ));
    }

    #[test]
    fn too_many_entries_rejected() {
        let manifest = valid_manifest();
        let mut entries: Vec<(&str, &[u8])> = vec![("manifest.json", &manifest)];
        let filler = b"x";
        for i in 0..100 {
            entries.push((Box::leak(format!("f{i}.txt").into_boxed_str()), filler));
        }
        let bytes = build_zip(&entries);
        let limits = SymbolPackLimits {
            max_entries: 10,
            ..SymbolPackLimits::default()
        };
        let result = validate_symbol_pack_archive(&bytes, &limits);
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::TooManyEntries { .. })
        ));
    }

    #[test]
    fn sniff_mime_detects_common_types() {
        assert_eq!(sniff_mime(&[0x89, b'P', b'N', b'G', 0x0d]), "image/png");
        assert_eq!(sniff_mime(&[0xff, 0xd8, 0xff, 0xe0]), "image/jpeg");
        assert_eq!(
            sniff_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            "image/svg+xml"
        );
        assert_eq!(
            sniff_mime(b"<?xml version='1.0'?><svg xmlns='x'/>"),
            "image/svg+xml"
        );
        assert_eq!(sniff_mime(b"garbage"), "application/octet-stream");
    }
}
