use latexsnipper_ast::{AssetId, MediaAsset};

/// Trait for resolving media assets to output-specific references.
pub trait AssetResolver {
    /// Resolve an asset ID to a string reference suitable for the target format.
    fn resolve(&self, asset_id: &AssetId) -> Option<String>;

    /// Get the full asset metadata.
    fn get_asset(&self, asset_id: &AssetId) -> Option<&MediaAsset>;
}

/// A simple asset resolver that uses a hash map.
pub struct SimpleAssetResolver {
    assets: std::collections::HashMap<AssetId, MediaAsset>,
}

impl SimpleAssetResolver {
    pub fn new() -> Self {
        Self {
            assets: std::collections::HashMap::new(),
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

impl AssetResolver for SimpleAssetResolver {
    fn resolve(&self, asset_id: &AssetId) -> Option<String> {
        self.assets.get(asset_id).map(|asset| match &asset.storage {
            latexsnipper_ast::AssetStorage::FilePath { path } => path.clone(),
            latexsnipper_ast::AssetStorage::Uri { uri } => uri.clone(),
            latexsnipper_ast::AssetStorage::InlineBase64 { data } => {
                format!("data:{};base64,{}", asset.mime_type.as_deref().unwrap_or("image/png"), data)
            }
            _ => format!("asset:{}", asset_id.0),
        })
    }

    fn get_asset(&self, asset_id: &AssetId) -> Option<&MediaAsset> {
        self.assets.get(asset_id)
    }
}