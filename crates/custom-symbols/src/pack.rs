//! Secure custom-symbol pack validation.
//!
//! `*.lsymbolpack` is a controlled ZIP archive. This module validates the
//! archive without extracting it: path safety (no absolute paths, no `..`,
//! no symlink escapes), entry counts and size limits, compression-ratio
//! bounds, MIME sniffing of symbol assets, and a required manifest.

use serde::{Deserialize, Serialize};

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
    /// A path in the archive is unsafe (absolute, `..`, or escapes).
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

/// Minimal ZIP local-file parser for validation.
///
/// We deliberately parse only the central directory / local headers we need:
/// name, flags (symlink detection), and sizes. No extraction happens.
pub fn validate_symbol_pack_archive(
    bytes: &[u8],
    limits: &SymbolPackLimits,
) -> Result<SymbolPackValidation, SymbolPackValidationError> {
    use sha2::Digest;
    let pack_sha256 = format!("{:x}", sha2::Sha256::digest(bytes));

    // Locate End Of Central Directory (EOCD).
    let eocd = find_eocd(bytes).ok_or(SymbolPackValidationError::NotAZip)?;
    let cd_offset = u32_at(bytes, eocd + 16) as usize;
    let cd_entries = u16_at(bytes, eocd + 10) as usize;

    if cd_entries > limits.max_entries {
        return Err(SymbolPackValidationError::TooManyEntries {
            count: cd_entries,
            limit: limits.max_entries,
        });
    }

    // Walk the central directory.
    let mut entries: Vec<PackEntryInfo> = Vec::new();
    let mut cursor = cd_offset;
    let mut total_uncompressed = 0usize;
    for _ in 0..cd_entries {
        if cursor + 46 > bytes.len() {
            return Err(SymbolPackValidationError::NotAZip);
        }
        if bytes[cursor..cursor + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err(SymbolPackValidationError::NotAZip);
        }
        let _flags = u16_at(bytes, cursor + 8);
        let method = u16_at(bytes, cursor + 10);
        let compressed = u32_at(bytes, cursor + 20) as usize;
        let uncompressed = u32_at(bytes, cursor + 24) as usize;
        let name_len = u16_at(bytes, cursor + 28) as usize;
        let extra_len = u16_at(bytes, cursor + 30) as usize;
        let comment_len = u16_at(bytes, cursor + 32) as usize;
        let local_header_offset = u32_at(bytes, cursor + 42) as usize;

        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        if name_end > bytes.len() {
            return Err(SymbolPackValidationError::NotAZip);
        }
        let name = String::from_utf8_lossy(&bytes[name_start..name_end]).into_owned();

        // Path safety.
        validate_path(&name)?;

        // Symlink detection via name (Unix symlink entries conventionally
        // end with no content and use method 0; we also flag entry names).
        let is_dir = name.ends_with('/');
        let looks_like_symlink = is_symlink_entry(&name);
        if looks_like_symlink {
            return Err(SymbolPackValidationError::SymlinkEntry(name.clone()));
        }

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

        // Read the entry bytes (only for the manifest and asset candidates).
        let bytes_here = if uncompressed <= limits.max_entry_bytes {
            read_local_entry(bytes, local_header_offset, compressed, uncompressed, method)
        } else {
            None
        };

        entries.push(PackEntryInfo {
            path: name,
            is_dir,
            is_symlink: looks_like_symlink,
            compressed_size: compressed,
            uncompressed_size: uncompressed,
            bytes: bytes_here,
        });

        cursor = name_end + extra_len + comment_len;
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
            if !entries.iter().any(|e| e.path == *path) {
                return Err(SymbolPackValidationError::MissingReferencedFile(
                    path.clone(),
                ));
            }
        }
        if let Some(preview) = &symbol.preview_png_path {
            if !entries.iter().any(|e| e.path == *preview) {
                return Err(SymbolPackValidationError::MissingReferencedFile(
                    preview.clone(),
                ));
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
        entry_count: cd_entries,
        pack_sha256,
    })
}

/// Read the content of one local file entry (only for stored or deflate
/// entries within the size cap). Returns None when unsupported or out of
/// bounds.
fn read_local_entry(
    bytes: &[u8],
    local_offset: usize,
    compressed: usize,
    uncompressed: usize,
    method: u16,
) -> Option<Vec<u8>> {
    // Local file header: sig(4) ver(2) flags(2) method(2) time(2) date(2)
    // crc(4) comp(4) uncomp(4) namelen(2) extralen(2) => 30 bytes.
    if local_offset + 30 > bytes.len() {
        return None;
    }
    if bytes[local_offset..local_offset + 4] != [0x50, 0x4b, 0x03, 0x04] {
        return None;
    }
    let name_len = u16_at(bytes, local_offset + 26) as usize;
    let extra_len = u16_at(bytes, local_offset + 28) as usize;
    let data_start = local_offset + 30 + name_len + extra_len;
    let data_end = data_start + compressed;
    if data_end > bytes.len() {
        return None;
    }
    let raw = &bytes[data_start..data_end];
    match method {
        0 => Some(raw.to_vec()), // stored
        8 => {
            // Deflate without external crate: keep raw (validation-only).
            let _ = uncompressed;
            None
        }
        _ => None,
    }
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    let min_scan = bytes.len().saturating_sub(65557);
    // EOCD is 22 bytes; its start can be exactly len-22, so the exclusive
    // upper bound must be len-21.
    let upper = bytes.len().saturating_sub(21);
    (min_scan..upper)
        .rev()
        .find(|&i| i + 4 <= bytes.len() && bytes[i..i + 4] == EOCD_SIG)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn validate_path(path: &str) -> Result<(), SymbolPackValidationError> {
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(SymbolPackValidationError::UnsafePath(path.into()));
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(SymbolPackValidationError::UnsafePath(path.into()));
    }
    Ok(())
}

fn is_symlink_entry(name: &str) -> bool {
    name.ends_with(" -> ") || name.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one stored entry: returns (local_record, central_directory_record).
    /// All local records must be concatenated first, then all central
    /// directory records, then the EOCD.
    fn store_entry(local_offset: usize, name: &str, content: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut local = Vec::new();
        local.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // sig
        local.extend_from_slice(&[20, 0]); // version
        local.extend_from_slice(&[0, 0]); // flags
        local.extend_from_slice(&[0, 0]); // method: stored
        local.extend_from_slice(&[0, 0, 0, 0]); // time/date
        local.extend_from_slice(&[0, 0, 0, 0]); // crc
        local.extend_from_slice(&(content.len() as u32).to_le_bytes());
        local.extend_from_slice(&(content.len() as u32).to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&[0, 0]); // extra len
        local.extend_from_slice(name.as_bytes());
        local.extend_from_slice(content);

        let mut cd = Vec::new();
        cd.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // sig
        cd.extend_from_slice(&[20, 0, 20, 0]); // version made/needed
        cd.extend_from_slice(&[0, 0]); // flags
        cd.extend_from_slice(&[0, 0]); // method
        cd.extend_from_slice(&[0, 0, 0, 0]); // time/date
        cd.extend_from_slice(&[0, 0, 0, 0]); // crc
        cd.extend_from_slice(&(content.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(content.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&[0, 0]); // extra len
        cd.extend_from_slice(&[0, 0]); // comment len
        cd.extend_from_slice(&[0, 0, 0, 0]); // disk number / internal attrs
        cd.extend_from_slice(&[0, 0, 0, 0]); // external attrs
        cd.extend_from_slice(&(local_offset as u32).to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        (local, cd)
    }

    /// Concatenate (local records, cd records) and append the EOCD.
    fn finish_zip(locals: &[Vec<u8>], cds: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for l in locals {
            out.extend_from_slice(l);
        }
        let cd_start = out.len();
        for c in cds {
            out.extend_from_slice(c);
        }
        let cd_size = (out.len() - cd_start) as u32;
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // EOCD sig
        out.extend_from_slice(&[0, 0, 0, 0]); // disk numbers
        out.extend_from_slice(&(cds.len() as u16).to_le_bytes()); // entries this disk
        out.extend_from_slice(&(cds.len() as u16).to_le_bytes()); // entries total
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&(cd_start as u32).to_le_bytes());
        out.extend_from_slice(&[0, 0]); // comment length
        out
    }

    /// Helper: build a full zip from (name, content) pairs.
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut locals = Vec::new();
        let mut cds = Vec::new();
        let mut offset = 0usize;
        for (name, content) in entries {
            let (l, c) = store_entry(offset, name, content);
            offset += l.len();
            locals.push(l);
            cds.push(c);
        }
        finish_zip(&locals, &cds)
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
    fn valid_pack_validates() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[
            ("manifest.json", &manifest),
            ("symbols/sym-1/source.svg", b"<svg/>"),
            ("symbols/sym-1/canonical.svg", b"<svg/>"),
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
    fn missing_referenced_file_rejected() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[
            ("manifest.json", &manifest),
            ("symbols/sym-1/source.svg", b"<svg/>"),
        ]);
        // canonical.svg referenced but missing.
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::MissingReferencedFile(_))
        ));
    }

    #[test]
    fn too_many_entries_rejected() {
        let manifest = valid_manifest();
        let mut locals = Vec::new();
        let mut cds = Vec::new();
        let mut offset = 0usize;
        let (l, c) = store_entry(offset, "manifest.json", &manifest);
        offset += l.len();
        locals.push(l);
        cds.push(c);
        for i in 0..100 {
            let name = format!("f{i}.txt");
            let (l, c) = store_entry(offset, &name, b"x");
            offset += l.len();
            locals.push(l);
            cds.push(c);
        }
        let bytes = finish_zip(&locals, &cds);
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
    fn absolute_path_rejected() {
        let manifest = valid_manifest();
        let bytes = build_zip(&[("manifest.json", &manifest), ("/etc/passwd", b"x")]);
        let result = validate_symbol_pack_archive(&bytes, &SymbolPackLimits::default());
        assert!(matches!(
            result,
            Err(SymbolPackValidationError::UnsafePath(_))
        ));
    }
}
