use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use latexsnipper_foundation::{Result, SnipperError};
use resvg::usvg;
use serde::{Deserialize, Serialize};

use crate::bundle::RenderDimensions;

/// Validation requirements applied to SVG input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SvgContentPolicy {
    /// Embedded raster images are allowed.
    AllowEmbeddedRaster,

    /// The SVG must contain vector content only.
    VectorOnly,
}

/// Metadata obtained after parsing an SVG.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgValidation {
    pub dimensions: RenderDimensions,
    pub has_raster_images: bool,
}

/// Canonical SVG text and its validated metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedSvg {
    pub svg: String,
    pub validation: SvgValidation,
}

/// Parse an SVG using the shared security policy.
///
/// String-based image hrefs are rejected instead of being resolved from the
/// host filesystem.
pub(crate) fn parse_svg_tree(
    input: &str,
    policy: SvgContentPolicy,
) -> Result<(usvg::Tree, SvgValidation)> {
    let external_image_reference_seen = Arc::new(AtomicBool::new(false));

    let external_image_reference_marker = Arc::clone(&external_image_reference_seen);

    let options = usvg::Options {
        // Never establish a filesystem base directory for SVG resources.
        resources_dir: None,

        // Data URLs remain available so self-contained SVG can embed images.
        // Arbitrary string hrefs are rejected instead of being interpreted as
        // local filesystem paths.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),

            resolve_string: Box::new(move |_href, _options| {
                external_image_reference_marker.store(true, Ordering::Relaxed);
                None
            }),
        },
        ..Default::default()
    };

    let tree = usvg::Tree::from_str(input, &options)
        .map_err(|error| SnipperError::Export(format!("Invalid SVG input: {error}")))?;

    if external_image_reference_seen.load(Ordering::Relaxed) {
        return Err(SnipperError::Export(
            "External SVG image references are not allowed".to_string(),
        ));
    }

    let has_raster_images = tree_contains_raster_images(&tree);

    if policy == SvgContentPolicy::VectorOnly && has_raster_images {
        return Err(SnipperError::Export(
            "Vector-only SVG contains embedded raster images".to_string(),
        ));
    }

    let size = tree.size();

    let validation = SvgValidation {
        dimensions: RenderDimensions {
            width_px: size.width(),
            height_px: size.height(),
            dpi: options.dpi,
        },
        has_raster_images,
    };

    Ok((tree, validation))
}

/// Validate SVG input without rewriting it.
pub fn validate_svg(input: &str, policy: SvgContentPolicy) -> Result<SvgValidation> {
    let (_, validation) = parse_svg_tree(input, policy)?;
    Ok(validation)
}

/// Parse and rewrite SVG into the normalized usvg representation.
pub fn normalize_svg(input: &str, policy: SvgContentPolicy) -> Result<NormalizedSvg> {
    let (tree, validation) = parse_svg_tree(input, policy)?;

    let svg = tree.to_string(&usvg::WriteOptions::default());

    Ok(NormalizedSvg { svg, validation })
}

fn tree_contains_raster_images(tree: &usvg::Tree) -> bool {
    group_contains_raster_images(tree.root())
}

fn group_contains_raster_images(group: &usvg::Group) -> bool {
    for node in group.children() {
        let contains_raster = match node {
            usvg::Node::Image(image) => match image.kind() {
                usvg::ImageKind::SVG(svg) => tree_contains_raster_images(svg),
                usvg::ImageKind::JPEG(_)
                | usvg::ImageKind::PNG(_)
                | usvg::ImageKind::GIF(_)
                | usvg::ImageKind::WEBP(_) => true,
            },
            usvg::Node::Group(child) => group_contains_raster_images(child),
            usvg::Node::Path(_) | usvg::Node::Text(_) => false,
        };

        if contains_raster {
            return true;
        }

        let mut subroot_contains_raster = false;

        node.subroots(|subroot| {
            if group_contains_raster_images(subroot) {
                subroot_contains_raster = true;
            }
        });

        if subroot_contains_raster {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_svg_is_rejected() {
        assert!(validate_svg("<svg>", SvgContentPolicy::AllowEmbeddedRaster).is_err());
    }

    #[test]
    fn external_image_reference_is_rejected() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="100"
              height="100">
              <image
                href="file:///tmp/private.png"
                width="100"
                height="100"/>
            </svg>
        "#;

        assert!(validate_svg(svg, SvgContentPolicy::AllowEmbeddedRaster).is_err());
    }

    #[test]
    fn vector_only_rejects_embedded_png() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="1"
              height="1">
              <image
                href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
                width="1"
                height="1"/>
            </svg>
        "#;

        assert!(validate_svg(svg, SvgContentPolicy::VectorOnly).is_err());
    }

    #[test]
    fn normalized_svg_can_be_parsed_again() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="200"
              height="100"
              viewBox="0 0 200 100">
              <rect
                width="200"
                height="100"/>
            </svg>
        "#;

        let normalized = normalize_svg(svg, SvgContentPolicy::VectorOnly).unwrap();

        assert_eq!(normalized.validation.dimensions.width_px, 200.0);

        validate_svg(&normalized.svg, SvgContentPolicy::VectorOnly).unwrap();
    }

    #[test]
    fn vector_only_allows_pure_vector_svg() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="100"
              height="100">
              <circle cx="50" cy="50" r="40"/>
            </svg>
        "#;

        let validation = validate_svg(svg, SvgContentPolicy::VectorOnly).unwrap();
        assert!(!validation.has_raster_images);
    }

    #[test]
    fn allow_embedded_raster_accepts_embedded_png() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="1"
              height="1">
              <image
                href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
                width="1"
                height="1"/>
            </svg>
        "#;

        let validation = validate_svg(svg, SvgContentPolicy::AllowEmbeddedRaster).unwrap();
        assert!(validation.has_raster_images);
    }
}
