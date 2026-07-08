use latexsnipper_ast::{AssetId, AssetStorage, MediaAsset};

/// Resolve an optional `AssetId` against the document's asset collection to get
/// the best available path or data-URI string for use in converter output.
///
/// Falls back through:
///   1. Resolved path from storage (FilePath, Uri)
///   2. Inline base64 data-URI
///   3. A placeholder string built from the asset ID
///   4. `""` if no asset_id is provided
pub fn resolve_asset_ref(assets: &[MediaAsset], asset_id: &Option<AssetId>) -> String {
    let id = match asset_id {
        Some(id) => id,
        None => return String::new(),
    };

    if let Some(asset) = assets.iter().find(|a| &a.id == id) {
        match &asset.storage {
            AssetStorage::FilePath { path } => path.clone(),
            AssetStorage::Uri { uri } => uri.clone(),
            AssetStorage::InlineBase64 { data } => {
                let mime = asset.mime_type.as_deref().unwrap_or("image/png");
                format!("data:{};base64,{}", mime, data)
            }
            AssetStorage::BytesRef { id: ref_id } => format!("asset:{}", ref_id),
            AssetStorage::OfficeRelationship { r_id, .. } => {
                format!("rId:{}", r_id)
            }
            _ => format!("assets/{}", id.0),
        }
    } else {
        format!("assets/{}", id.0)
    }
}

/// Build an `<img>` tag for an inline image, using asset metadata when available.
pub fn resolve_image_html(assets: &[MediaAsset], asset_id: &Option<AssetId>, alt: &str) -> String {
    let src = resolve_asset_ref(assets, asset_id);
    if src.is_empty() {
        "<img src=\"image.png\" alt=\"image\">".to_string()
    } else {
        format!("<img src=\"{}\" alt=\"{}\">", src, alt)
    }
}

/// Build a Markdown image reference `![alt](src)`.
pub fn resolve_image_markdown(
    assets: &[MediaAsset],
    asset_id: &Option<AssetId>,
    alt: &str,
) -> String {
    let src = resolve_asset_ref(assets, asset_id);
    if src.is_empty() {
        format!("![{}](image.png)", alt)
    } else {
        format!("![{}]({})", alt, src)
    }
}

/// Build a LaTeX `\includegraphics{path}`.
pub fn resolve_image_latex(assets: &[MediaAsset], asset_id: &Option<AssetId>) -> String {
    let src = resolve_asset_ref(assets, asset_id);
    if src.is_empty() {
        "\\includegraphics{image}".to_string()
    } else {
        format!("\\includegraphics{{{}}}", src)
    }
}

/// Build a Typst `#image("path")`.
pub fn resolve_image_typst(assets: &[MediaAsset], asset_id: &Option<AssetId>) -> String {
    let src = resolve_asset_ref(assets, asset_id);
    if src.is_empty() {
        "#image(\"image.png\")".to_string()
    } else {
        format!("#image(\"{}\")", src)
    }
}
