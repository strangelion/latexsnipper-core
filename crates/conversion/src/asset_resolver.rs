use latexsnipper_ast::{
    AssetExporter, AssetId, AssetReferenceResolver, AssetStore, ExportedAsset, MediaAsset,
};
use std::collections::HashMap;
use std::path::Path;

/// Minimal base64 decode — handles standard base64 without padding.
/// Strips data URI prefix, whitespace, and URL-safe chars before decoding.
fn simple_base64_decode(data: &str) -> Result<Vec<u8>, String> {
    // Strip data URI prefix
    let data = if let Some(pos) = data.find(",") {
        let prefix = &data[..pos];
        if prefix.contains("base64") || prefix.contains(";") {
            &data[pos + 1..]
        } else {
            data
        }
    } else {
        data
    };

    // Strip whitespace, URL-safe chars
    let data: String = data.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let data = data.replace('-', "+").replace('_', "/");

    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = data.trim_end_matches('=');
    let mut result = Vec::new();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in data.as_bytes() {
        if let Some(pos) = CHARS.iter().position(|&c| c == b) {
            buf = (buf << 6) | pos as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                result.push((buf >> bits) as u8);
                buf &= (1u32 << bits) - 1;
            }
        } else {
            return Err(format!("Invalid base64 character: {}", b as char));
        }
    }
    if bits > 0 && buf != 0 {
        // Check for leftover non-zero bits
        if bits >= 6 || buf != 0 {
            // Still have valid data, but we can't decode partial groups
        }
    }
    Ok(result)
}

/// A simple asset resolver that uses a hash map.
pub struct SimpleAssetResolver {
    assets: HashMap<AssetId, MediaAsset>,
}

impl SimpleAssetResolver {
    pub fn new() -> Self {
        Self {
            assets: HashMap::new(),
        }
    }

    pub fn add_asset(&mut self, asset: MediaAsset) {
        self.assets.insert(asset.id.clone(), asset);
    }
}

impl Default for SimpleAssetResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetStore for SimpleAssetResolver {
    fn get_bytes(&self, id: &AssetId) -> std::result::Result<Vec<u8>, String> {
        self.assets
            .get(id)
            .map(|asset| match &asset.storage {
                latexsnipper_ast::AssetStorage::InlineBase64 { data } => simple_base64_decode(data),
                latexsnipper_ast::AssetStorage::FilePath { path } => {
                    std::fs::read(path).map_err(|e| format!("file read error: {}", e))
                }
                _ => Err(format!(
                    "cannot get bytes from storage type for asset {}",
                    id.0
                )),
            })
            .unwrap_or(Err(format!("asset {} not found", id.0)))
    }

    fn get_asset(&self, id: &AssetId) -> Option<&MediaAsset> {
        self.assets.get(id)
    }
}

impl AssetReferenceResolver for SimpleAssetResolver {
    fn resolve_reference(&self, id: &AssetId) -> std::result::Result<String, String> {
        self.assets
            .get(id)
            .map(|asset| match &asset.storage {
                latexsnipper_ast::AssetStorage::FilePath { path } => Ok(path.clone()),
                latexsnipper_ast::AssetStorage::Uri { uri } => Ok(uri.clone()),
                latexsnipper_ast::AssetStorage::InlineBase64 { data } => Ok(format!(
                    "data:{};base64,{}",
                    asset.mime_type.as_deref().unwrap_or("image/png"),
                    data
                )),
                _ => Ok(format!("asset:{}", id.0)),
            })
            .unwrap_or(Err(format!("asset {} not found", id.0)))
    }
}

impl AssetExporter for SimpleAssetResolver {
    fn export_asset(
        &self,
        id: &AssetId,
        target_dir: &Path,
    ) -> std::result::Result<ExportedAsset, String> {
        let asset = self
            .assets
            .get(id)
            .ok_or_else(|| format!("asset {} not found", id.0))?;
        let bytes = self.get_bytes(id)?;
        let ext = match &asset.format {
            latexsnipper_ast::AssetFormat::Png => "png",
            latexsnipper_ast::AssetFormat::Jpeg => "jpg",
            latexsnipper_ast::AssetFormat::Svg => "svg",
            latexsnipper_ast::AssetFormat::Gif => "gif",
            latexsnipper_ast::AssetFormat::Webp => "webp",
            latexsnipper_ast::AssetFormat::Bmp => "bmp",
            latexsnipper_ast::AssetFormat::Tiff => "tiff",
            _ => "bin",
        };
        let filename = format!("{}.{}", id.0, ext);
        let dest = target_dir.join(&filename);
        std::fs::create_dir_all(target_dir).map_err(|e| format!("create dir error: {}", e))?;
        std::fs::write(&dest, &bytes).map_err(|e| format!("write error: {}", e))?;
        Ok(ExportedAsset {
            asset_id: id.clone(),
            relative_path: filename,
            format: asset.format,
            mime_type: asset.mime_type.clone(),
            checksum_sha256: asset.checksum.clone(),
        })
    }
}
