use latexsnipper_ast::{
    AssetExporter, AssetId, AssetReferenceResolver, AssetStore, ExportedAsset, MediaAsset,
};
use std::collections::HashMap;
use std::path::Path;

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
                latexsnipper_ast::AssetStorage::InlineBase64 { data } => {
                    // Return raw base64 bytes; caller may decode if needed.
                    Ok(data.as_bytes().to_vec())
                }
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
            checksum_sha256: asset.checksum_sha256.clone(),
        })
    }
}
