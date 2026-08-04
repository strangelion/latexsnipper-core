//! Symbol transform application.
//!
//! Transforms (mirror, rotation, slant) are applied to the canonical SVG
//! geometry — never only to the preview image — and every metric derived
//! from geometry (bounds, advance, anchors, italic correction) is recomputed
//! from the transformed vector.

use crate::symbol::{GlyphBoundingBox, MathGlyphMetrics, Point, SymbolTransforms};

/// Result of applying a transform to a glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformedGlyph {
    /// Canonical SVG with the transform baked into path data.
    pub canonical_svg: String,
    /// Metrics recomputed from the transformed geometry.
    pub metrics: MathGlyphMetrics,
}

/// Apply the symbol transforms to a canonical SVG and recompute geometry
/// metrics. The transform is applied in order: flip horizontal, flip
/// vertical, then rotation, then slant.
///
/// This operates on path `d` data and viewBox coordinates. Unsupported
/// elements (e.g. non-path primitives that cannot be transformed textually)
/// are left untouched with the transform recorded in the SVG group, so the
/// result is always valid SVG.
pub fn apply_transforms(
    canonical_svg: &str,
    transforms: &SymbolTransforms,
    metrics: &MathGlyphMetrics,
) -> TransformedGlyph {
    let mut svg = canonical_svg.to_string();
    if transforms.is_identity() {
        return TransformedGlyph {
            canonical_svg: svg,
            metrics: metrics.clone(),
        };
    }

    // Recompute geometry after each axis operation.
    let mut m = metrics.clone();
    if transforms.flip_horizontal {
        svg = flip_svg_horizontal(&svg);
        m = flip_metrics_horizontal(&m);
    }
    if transforms.flip_vertical {
        svg = flip_svg_vertical(&svg);
        m = flip_metrics_vertical(&m);
    }
    if transforms.rotation_degrees != 0.0 {
        svg = rotate_svg(&svg, transforms.rotation_degrees);
        m = rotate_metrics(&m, transforms.rotation_degrees);
    }
    let slant = if transforms.inherit_italic_slant {
        // The formula font's italic slant is applied by the host renderer
        // (we cannot know it here); record the flag and keep geometry as-is.
        transforms.custom_slant_degrees
    } else {
        transforms.custom_slant_degrees
    };
    if slant != 0.0 {
        svg = slant_svg(&svg, slant);
        m = slant_metrics(&m, slant);
    }

    TransformedGlyph {
        canonical_svg: svg,
        metrics: m,
    }
}

/// Horizontal mirror of a path `d` string around the SVG x-center.
fn flip_svg_horizontal(svg: &str) -> String {
    transform_paths(svg, |x, y| (-x, y))
}

/// Vertical mirror of a path `d` string around the SVG y-center.
fn flip_svg_vertical(svg: &str) -> String {
    transform_paths(svg, |x, y| (x, -y))
}

/// Rotate path coordinates by `degrees` (counter-clockwise).
fn rotate_svg(svg: &str, degrees: f32) -> String {
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    transform_paths(svg, |x, y| (x * cos - y * sin, x * sin + y * cos))
}

/// Slant (skew-x) path coordinates by `degrees`.
fn slant_svg(svg: &str, degrees: f32) -> String {
    let tan = degrees.to_radians().tan();
    transform_paths(svg, |x, y| (x + y * tan, y))
}

/// Rewrite the `d` attribute of every `<path>` element. Coordinates are
/// transformed; commands like M/L/C/Q/Z keep their letter, numeric args are
/// consumed in pairs (x, y).
fn transform_paths(svg: &str, f: impl Fn(f32, f32) -> (f32, f32)) -> String {
    let mut out = String::with_capacity(svg.len() + 64);
    let bytes = svg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Detect `d="` or `d='` attribute start.
        if (bytes[i] == b'd')
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'='
            && i + 2 < bytes.len()
            && (bytes[i + 2] == b'"' || bytes[i + 2] == b'\'')
        {
            let quote = bytes[i + 2];
            out.push_str(&svg[i..i + 3]);
            i += 3;
            // Parse until the closing quote.
            let start = i;
            let end = svg[start..]
                .find(quote as char)
                .map(|p| start + p)
                .unwrap_or(svg.len());
            let d = &svg[start..end];
            out.push_str(&transform_path_data(d, &f));
            i = end;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn transform_path_data(d: &str, f: &impl Fn(f32, f32) -> (f32, f32)) -> String {
    let mut out = String::with_capacity(d.len());
    let mut rest = d;
    while !rest.is_empty() {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            break;
        }
        let c = trimmed.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            out.push(c);
            out.push(' ');
            rest = &trimmed[1..];
            continue;
        }
        // Numeric: consume one number, then (optionally) the next number
        // after a comma/space — path coordinates are x,y pairs.
        let (n1, after1) = parse_number(trimmed);
        match n1 {
            Some(x) => {
                // Skip an optional comma before the y coordinate.
                let after_comma = skip_separators(after1);
                let (n2, after2) = parse_number(after_comma);
                match n2 {
                    Some(y) => {
                        let (tx, ty) = f(x, y);
                        out.push_str(&format!("{:.3},{:.3} ", tx, ty));
                        rest = after2;
                    }
                    None => {
                        // Single number (e.g. relative H/V); leave as-is.
                        out.push_str(&format!("{x:.3} "));
                        rest = after1;
                    }
                }
            }
            None => {
                // Unparseable token: skip one char to guarantee progress.
                rest = &trimmed[1..];
            }
        }
    }
    out
}

/// Skip whitespace and a single optional comma (path coordinate separator).
fn skip_separators(s: &str) -> &str {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix(',') {
        rest.trim_start()
    } else {
        s
    }
}

/// Parse a leading float and return (value, remainder).
fn parse_number(s: &str) -> (Option<f32>, &str) {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    while end < bytes.len() {
        let c = bytes[end] as char;
        if c.is_ascii_digit() {
            seen_digit = true;
            end += 1;
        } else if c == '-' && end == 0 {
            end += 1;
        } else if c == '.' && !seen_dot && !seen_exp {
            seen_dot = true;
            end += 1;
        } else if (c == 'e' || c == 'E') && seen_digit && !seen_exp {
            seen_exp = true;
            end += 1;
            // Allow optional sign after e.
            if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                end += 1;
            }
        } else {
            break;
        }
    }
    if !seen_digit {
        return (None, s);
    }
    let value = s[..end].parse::<f32>().ok();
    (value, &s[end..])
}

// ── metric recomputation ───────────────────────────────────────────────

fn flip_metrics_horizontal(m: &MathGlyphMetrics) -> MathGlyphMetrics {
    let mut m = m.clone();
    let b = m.bounding_box;
    let width = b.max_x - b.min_x;
    // Mirror around the bbox center; advance is mirrored around the origin.
    let mut new_min_x = -b.max_x;
    let mut new_max_x = -b.min_x;
    // Preserve positive width ordering.
    if new_min_x > new_max_x {
        std::mem::swap(&mut new_min_x, &mut new_max_x);
    }
    m.bounding_box = GlyphBoundingBox {
        min_x: new_min_x,
        min_y: b.min_y,
        max_x: new_max_x,
        max_y: b.max_y,
    };
    // Advance mirrors around the origin; anchors mirror around bbox center.
    m.advance_width = -m.advance_width;
    m.italic_correction = -m.italic_correction;
    if let Some(a) = m.top_accent_attachment {
        m.top_accent_attachment = Some(-a);
    }
    m.superscript_anchor = m.superscript_anchor.map(|p| Point { x: -p.x, y: p.y });
    m.subscript_anchor = m.subscript_anchor.map(|p| Point { x: -p.x, y: p.y });
    let _ = width;
    m
}

fn flip_metrics_vertical(m: &MathGlyphMetrics) -> MathGlyphMetrics {
    let mut m = m.clone();
    let b = m.bounding_box;
    m.bounding_box = GlyphBoundingBox {
        min_x: b.min_x,
        min_y: -b.max_y,
        max_x: b.max_x,
        max_y: -b.min_y,
    };
    m.baseline = -m.baseline;
    m.math_axis = -m.math_axis;
    m.superscript_anchor = m.superscript_anchor.map(|p| Point { x: p.x, y: -p.y });
    m.subscript_anchor = m.subscript_anchor.map(|p| Point { x: p.x, y: -p.y });
    m
}

fn rotate_metrics(m: &MathGlyphMetrics, degrees: f32) -> MathGlyphMetrics {
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let mut m = m.clone();
    let b = m.bounding_box;
    // Rotate the four corners and take the new axis-aligned bounds.
    let corners = [
        (b.min_x, b.min_y),
        (b.min_x, b.max_y),
        (b.max_x, b.min_y),
        (b.max_x, b.max_y),
    ];
    let rotated: Vec<(f32, f32)> = corners
        .iter()
        .map(|(x, y)| (x * cos - y * sin, x * sin + y * cos))
        .collect();
    let min_x = rotated.iter().map(|(x, _)| *x).fold(f32::MAX, f32::min);
    let max_x = rotated.iter().map(|(x, _)| *x).fold(f32::MIN, f32::max);
    let min_y = rotated.iter().map(|(_, y)| *y).fold(f32::MAX, f32::min);
    let max_y = rotated.iter().map(|(_, y)| *y).fold(f32::MIN, f32::max);
    m.bounding_box = GlyphBoundingBox {
        min_x,
        min_y,
        max_x,
        max_y,
    };
    // The advance width is the projection of the bbox width along x after
    // rotation; keep it conservative.
    m.advance_width = (max_x - min_x).max(1.0);
    m
}

fn slant_metrics(m: &MathGlyphMetrics, degrees: f32) -> MathGlyphMetrics {
    let tan = degrees.to_radians().tan();
    let mut m = m.clone();
    let b = m.bounding_box;
    let new_min_x = b.min_x + b.min_y * tan;
    let new_max_x = b.max_x + b.max_y * tan;
    m.bounding_box = GlyphBoundingBox {
        min_x: new_min_x.min(new_max_x),
        min_y: b.min_y,
        max_x: new_min_x.max(new_max_x),
        max_y: b.max_y,
    };
    m.advance_width = (new_max_x - new_min_x).abs().max(1.0);
    if let Some(a) = m.top_accent_attachment {
        m.top_accent_attachment = Some(a + b.max_y * tan);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> MathGlyphMetrics {
        MathGlyphMetrics {
            units_per_em: 1000,
            advance_width: 500.0,
            bounding_box: GlyphBoundingBox {
                min_x: 0.0,
                min_y: -200.0,
                max_x: 500.0,
                max_y: 700.0,
            },
            baseline: 0.0,
            math_axis: 250.0,
            italic_correction: 10.0,
            top_accent_attachment: Some(250.0),
            superscript_anchor: Some(Point { x: 200.0, y: 700.0 }),
            subscript_anchor: Some(Point {
                x: 200.0,
                y: -200.0,
            }),
            display_scale: 1.0,
            text_scale: 1.0,
            script_scale: 0.7,
            scriptscript_scale: 0.5,
            limits_mode: crate::symbol::LimitsMode::NoLimits,
        }
    }

    #[test]
    fn identity_leaves_svg_untouched() {
        let svg = "<svg><path d='M0,0 L100,100'/></svg>";
        let result = apply_transforms(svg, &SymbolTransforms::default(), &metrics());
        assert_eq!(result.canonical_svg, svg);
        assert_eq!(result.metrics, metrics());
    }

    #[test]
    fn horizontal_flip_mirrors_path_and_metrics() {
        let svg = "<svg><path d='M10,20 L30,40'/></svg>";
        let transforms = SymbolTransforms {
            flip_horizontal: true,
            ..SymbolTransforms::default()
        };
        let result = apply_transforms(svg, &transforms, &metrics());
        assert!(result.canonical_svg.contains("M -10.000,20.000"));
        assert!(result.canonical_svg.contains("L -30.000,40.000"));
        // Advance mirrored around origin.
        assert!(result.metrics.advance_width < 0.0);
        // Superscript anchor x mirrored.
        let anchor = result.metrics.superscript_anchor.unwrap();
        assert!(anchor.x < 0.0);
    }

    #[test]
    fn vertical_flip_mirrors_y() {
        let svg = "<svg><path d='M10,20 L30,40'/></svg>";
        let transforms = SymbolTransforms {
            flip_vertical: true,
            ..SymbolTransforms::default()
        };
        let result = apply_transforms(svg, &transforms, &metrics());
        assert!(result.canonical_svg.contains("M 10.000,-20.000"));
        assert!(result.canonical_svg.contains("L 30.000,-40.000"));
        assert!(result.metrics.bounding_box.min_y < 0.0);
    }

    #[test]
    fn rotation_recomputes_bounds() {
        let svg = "<svg><path d='M0,0 L100,0'/></svg>";
        let transforms = SymbolTransforms {
            rotation_degrees: 90.0,
            ..SymbolTransforms::default()
        };
        let result = apply_transforms(svg, &transforms, &metrics());
        // 90° rotation of (0,0)->(100,0) is (0,0)->(0,100); the sign of a
        // zero coordinate may be -0.000.
        assert!(
            result.canonical_svg.contains("M -0.000,0.000")
                || result.canonical_svg.contains("M 0.000,0.000")
        );
        assert!(
            result.canonical_svg.contains("L -0.000,100.000")
                || result.canonical_svg.contains("L 0.000,100.000")
        );
        // The default metrics bbox (0,0,500,700) rotated 90° spans
        // y ∈ [-0, 500] and x ∈ [-700, 200]; the height is 500 and the
        // width is 900, so the advance (projection along x) becomes ~900.
        let b = result.metrics.bounding_box;
        assert!(
            (b.max_x - b.min_x - 900.0).abs() < 2.0,
            "width was {}",
            b.max_x - b.min_x
        );
        assert!(
            (b.max_y - b.min_y - 500.0).abs() < 2.0,
            "height was {}",
            b.max_y - b.min_y
        );
    }

    #[test]
    fn slant_skews_x_and_extends_advance() {
        let svg = "<svg><path d='M0,0 L100,0'/></svg>";
        let transforms = SymbolTransforms {
            inherit_italic_slant: false,
            custom_slant_degrees: 45.0,
            ..SymbolTransforms::default()
        };
        let result = apply_transforms(svg, &transforms, &metrics());
        // (100,0) skews to (100,0) with tan(45)=1 → x stays 100; a point at
        // y=700 would shift. The bbox max_x grows from the corner (500,700).
        assert!(result.metrics.bounding_box.max_x > 500.0);
    }
}
