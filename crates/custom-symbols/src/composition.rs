//! Safe, versioned composition model for the visual custom-symbol editor.
//!
//! A custom symbol may be built from existing symbol snapshots or from a
//! small set of inert drawing primitives. The model contains no scripts,
//! external URLs or arbitrary embedded markup. Hosts can therefore expose a
//! visual canvas without turning symbol packs into an executable plugin API.

use serde::{Deserialize, Serialize};

use crate::symbol::{GlyphBoundingBox, MathGlyphMetrics, Point};

pub const COMPOSITION_SCHEMA_VERSION: u32 = 1;
pub const MAX_COMPOSITION_LAYERS: usize = 128;
pub const MAX_PATH_DATA_BYTES: usize = 16 * 1024;
pub const MAX_FORMULA_SOURCE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionTransform {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation_degrees: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

impl Default for CompositionTransform {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
        }
    }
}

impl CompositionTransform {
    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("translate_x", self.translate_x),
            ("translate_y", self.translate_y),
            ("scale_x", self.scale_x),
            ("scale_y", self.scale_y),
            ("rotation_degrees", self.rotation_degrees),
        ] {
            if !value.is_finite() {
                return Err(format!("composition {name} must be finite"));
            }
        }
        if self.scale_x.abs() < 0.01
            || self.scale_y.abs() < 0.01
            || self.scale_x.abs() > 100.0
            || self.scale_y.abs() > 100.0
        {
            return Err("composition scale must be within 0.01..=100".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DrawingPrimitive {
    Line {
        from: Point,
        to: Point,
        stroke_width: f32,
    },
    Rectangle {
        bounds: GlyphBoundingBox,
        corner_radius: f32,
        stroke_width: f32,
        filled: bool,
    },
    Ellipse {
        center: Point,
        radius_x: f32,
        radius_y: f32,
        stroke_width: f32,
        filled: bool,
    },
    /// Sanitized SVG path data only; elements, attributes and URLs are not
    /// accepted here. Hosts must render this as a path in the canonical SVG.
    Path {
        path_data: String,
        bounds: GlyphBoundingBox,
        stroke_width: f32,
        filled: bool,
    },
}

impl DrawingPrimitive {
    pub fn bounds(&self) -> GlyphBoundingBox {
        match self {
            Self::Line {
                from,
                to,
                stroke_width,
            } => {
                let pad = stroke_width.max(0.0) / 2.0;
                GlyphBoundingBox {
                    min_x: from.x.min(to.x) - pad,
                    min_y: from.y.min(to.y) - pad,
                    max_x: from.x.max(to.x) + pad,
                    max_y: from.y.max(to.y) + pad,
                }
            }
            Self::Rectangle {
                bounds,
                stroke_width,
                ..
            }
            | Self::Path {
                bounds,
                stroke_width,
                ..
            } => expand_bounds(*bounds, stroke_width.max(0.0) / 2.0),
            Self::Ellipse {
                center,
                radius_x,
                radius_y,
                stroke_width,
                ..
            } => {
                let pad = stroke_width.max(0.0) / 2.0;
                GlyphBoundingBox {
                    min_x: center.x - radius_x.abs() - pad,
                    min_y: center.y - radius_y.abs() - pad,
                    max_x: center.x + radius_x.abs() + pad,
                    max_y: center.y + radius_y.abs() + pad,
                }
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        let bounds = self.bounds();
        validate_bounds(bounds)?;
        match self {
            Self::Line { stroke_width, .. }
            | Self::Rectangle { stroke_width, .. }
            | Self::Ellipse { stroke_width, .. }
            | Self::Path { stroke_width, .. } => {
                if !stroke_width.is_finite() || *stroke_width < 0.0 || *stroke_width > 1_000.0 {
                    return Err("primitive stroke width is invalid".into());
                }
            }
        }
        if let Self::Path { path_data, .. } = self {
            if path_data.is_empty() || path_data.len() > MAX_PATH_DATA_BYTES {
                return Err("primitive path data is empty or too large".into());
            }
            if !path_data.chars().all(|ch| {
                ch.is_ascii_digit()
                    || ch.is_ascii_whitespace()
                    || matches!(
                        ch,
                        'M' | 'm'
                            | 'L'
                            | 'l'
                            | 'H'
                            | 'h'
                            | 'V'
                            | 'v'
                            | 'C'
                            | 'c'
                            | 'S'
                            | 's'
                            | 'Q'
                            | 'q'
                            | 'T'
                            | 't'
                            | 'A'
                            | 'a'
                            | 'Z'
                            | 'z'
                            | '.'
                            | ','
                            | '-'
                            | '+'
                            | 'e'
                            | 'E'
                    )
            }) {
                return Err("primitive path data contains unsupported content".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompositionLayerSource {
    Symbol {
        symbol_id: String,
        pack_id: Option<String>,
        metrics_snapshot: MathGlyphMetrics,
    },
    Primitive {
        primitive: DrawingPrimitive,
    },
    /// A locally rendered mathematical fragment. The source is preserved so
    /// hosts can render it with their bundled math engine after a round-trip;
    /// executable markup and external resources are never embedded here.
    Formula {
        latex: String,
        metrics_snapshot: MathGlyphMetrics,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionLayer {
    pub layer_id: String,
    pub name: String,
    pub source: CompositionLayerSource,
    pub transform: CompositionTransform,
    pub opacity: f32,
    /// Optional fixed sRGB color. `None` keeps the host's current math color.
    /// Only six-digit hex is accepted so the value remains inert and portable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub z_index: i32,
    pub visible: bool,
}

impl CompositionLayer {
    fn source_bounds(&self) -> GlyphBoundingBox {
        match &self.source {
            CompositionLayerSource::Symbol {
                metrics_snapshot, ..
            } => metrics_snapshot.bounding_box,
            CompositionLayerSource::Primitive { primitive } => primitive.bounds(),
            CompositionLayerSource::Formula {
                metrics_snapshot, ..
            } => metrics_snapshot.bounding_box,
        }
    }

    fn validate(&self, owner_symbol_id: &str) -> Result<(), String> {
        if self.layer_id.trim().is_empty() {
            return Err("composition layer id must not be empty".into());
        }
        self.transform.validate()?;
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err("composition opacity must be within 0..=1".into());
        }
        if let Some(color) = &self.color {
            let bytes = color.as_bytes();
            if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err("composition color must be a six-digit hex value".into());
            }
        }
        match &self.source {
            CompositionLayerSource::Symbol {
                symbol_id,
                metrics_snapshot,
                ..
            } => {
                if symbol_id.trim().is_empty() || symbol_id == owner_symbol_id {
                    return Err("composition symbol layer is empty or self-referential".into());
                }
                metrics_snapshot.validate()?;
            }
            CompositionLayerSource::Primitive { primitive } => primitive.validate()?,
            CompositionLayerSource::Formula {
                latex,
                metrics_snapshot,
            } => {
                let latex = latex.trim();
                if latex.is_empty() || latex.len() > MAX_FORMULA_SOURCE_BYTES {
                    return Err("composition formula source is empty or too large".into());
                }
                if latex
                    .chars()
                    .any(|ch| ch.is_control() && !ch.is_ascii_whitespace())
                {
                    return Err("composition formula source contains control characters".into());
                }
                metrics_snapshot.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathGlyphComposition {
    pub schema_version: u32,
    pub layers: Vec<CompositionLayer>,
    pub snap_to_grid: bool,
    pub grid_size: f32,
    pub auto_fit_canvas: bool,
}

impl Default for MathGlyphComposition {
    fn default() -> Self {
        Self {
            schema_version: COMPOSITION_SCHEMA_VERSION,
            layers: Vec::new(),
            snap_to_grid: true,
            grid_size: 25.0,
            auto_fit_canvas: true,
        }
    }
}

impl MathGlyphComposition {
    pub fn validate(&self, owner_symbol_id: &str) -> Result<(), String> {
        if self.schema_version != COMPOSITION_SCHEMA_VERSION {
            return Err("unsupported composition schema version".into());
        }
        if self.layers.is_empty() || self.layers.len() > MAX_COMPOSITION_LAYERS {
            return Err(format!(
                "composition must contain 1..={MAX_COMPOSITION_LAYERS} layers"
            ));
        }
        if !self.grid_size.is_finite() || self.grid_size <= 0.0 || self.grid_size > 1_000.0 {
            return Err("composition grid size is invalid".into());
        }
        let mut layer_ids = std::collections::HashSet::new();
        for layer in &self.layers {
            layer.validate(owner_symbol_id)?;
            if !layer_ids.insert(&layer.layer_id) {
                return Err("composition layer ids must be unique".into());
            }
        }
        self.visible_bounds()
            .ok_or_else(|| "composition has no visible geometry".to_string())?;
        Ok(())
    }

    pub fn visible_bounds(&self) -> Option<GlyphBoundingBox> {
        self.layers
            .iter()
            .filter(|layer| layer.visible && layer.opacity > 0.0)
            .map(transformed_layer_bounds)
            .reduce(union_bounds)
    }

    /// Recompute the symbol metrics from the visible composition. Baseline,
    /// math axis, style scales and limits mode remain explicit editor inputs;
    /// geometric width/bounds and anchors are clamped to the resulting ink.
    pub fn recompute_metrics(
        &self,
        template: &MathGlyphMetrics,
    ) -> Result<MathGlyphMetrics, String> {
        let bounds = self
            .visible_bounds()
            .ok_or_else(|| "composition has no visible geometry".to_string())?;
        validate_bounds(bounds)?;
        let mut metrics = template.clone();
        metrics.bounding_box = bounds;
        metrics.advance_width = (bounds.max_x - bounds.min_x).max(1.0);
        metrics.top_accent_attachment = metrics
            .top_accent_attachment
            .map(|x| x.clamp(bounds.min_x, bounds.max_x));
        metrics.superscript_anchor = metrics
            .superscript_anchor
            .map(|point| clamp_point(point, bounds));
        metrics.subscript_anchor = metrics
            .subscript_anchor
            .map(|point| clamp_point(point, bounds));
        metrics.validate()?;
        Ok(metrics)
    }
}

fn transformed_layer_bounds(layer: &CompositionLayer) -> GlyphBoundingBox {
    let bounds = layer.source_bounds();
    let transform = &layer.transform;
    let flip_x = if transform.flip_horizontal { -1.0 } else { 1.0 };
    let flip_y = if transform.flip_vertical { -1.0 } else { 1.0 };
    let radians = transform.rotation_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let corners = [
        Point {
            x: bounds.min_x,
            y: bounds.min_y,
        },
        Point {
            x: bounds.min_x,
            y: bounds.max_y,
        },
        Point {
            x: bounds.max_x,
            y: bounds.min_y,
        },
        Point {
            x: bounds.max_x,
            y: bounds.max_y,
        },
    ];
    corners
        .into_iter()
        .map(|point| {
            let x = point.x * transform.scale_x * flip_x;
            let y = point.y * transform.scale_y * flip_y;
            Point {
                x: x * cos - y * sin + transform.translate_x,
                y: x * sin + y * cos + transform.translate_y,
            }
        })
        .fold(None::<GlyphBoundingBox>, |bounds, point| {
            Some(match bounds {
                None => GlyphBoundingBox {
                    min_x: point.x,
                    min_y: point.y,
                    max_x: point.x,
                    max_y: point.y,
                },
                Some(mut bounds) => {
                    bounds.min_x = bounds.min_x.min(point.x);
                    bounds.min_y = bounds.min_y.min(point.y);
                    bounds.max_x = bounds.max_x.max(point.x);
                    bounds.max_y = bounds.max_y.max(point.y);
                    bounds
                }
            })
        })
        .expect("a bounding box always has four corners")
}

fn expand_bounds(bounds: GlyphBoundingBox, padding: f32) -> GlyphBoundingBox {
    GlyphBoundingBox {
        min_x: bounds.min_x - padding,
        min_y: bounds.min_y - padding,
        max_x: bounds.max_x + padding,
        max_y: bounds.max_y + padding,
    }
}

fn union_bounds(left: GlyphBoundingBox, right: GlyphBoundingBox) -> GlyphBoundingBox {
    GlyphBoundingBox {
        min_x: left.min_x.min(right.min_x),
        min_y: left.min_y.min(right.min_y),
        max_x: left.max_x.max(right.max_x),
        max_y: left.max_y.max(right.max_y),
    }
}

fn validate_bounds(bounds: GlyphBoundingBox) -> Result<(), String> {
    for value in [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y] {
        if !value.is_finite() {
            return Err("composition bounds must be finite".into());
        }
    }
    if bounds.max_x <= bounds.min_x || bounds.max_y <= bounds.min_y {
        return Err("composition bounds must have positive extent".into());
    }
    Ok(())
}

fn clamp_point(point: Point, bounds: GlyphBoundingBox) -> Point {
    Point {
        x: point.x.clamp(bounds.min_x, bounds.max_x),
        y: point.y.clamp(bounds.min_y, bounds.max_y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(id: &str, x: f32) -> CompositionLayer {
        CompositionLayer {
            layer_id: id.to_string(),
            name: id.to_string(),
            source: CompositionLayerSource::Primitive {
                primitive: DrawingPrimitive::Rectangle {
                    bounds: GlyphBoundingBox {
                        min_x: 0.0,
                        min_y: 0.0,
                        max_x: 100.0,
                        max_y: 200.0,
                    },
                    corner_radius: 0.0,
                    stroke_width: 0.0,
                    filled: true,
                },
            },
            transform: CompositionTransform {
                translate_x: x,
                ..CompositionTransform::default()
            },
            opacity: 1.0,
            color: None,
            z_index: 0,
            visible: true,
        }
    }

    #[test]
    fn multiple_layers_recompute_union_metrics() {
        let composition = MathGlyphComposition {
            layers: vec![layer("left", 0.0), layer("right", 150.0)],
            ..MathGlyphComposition::default()
        };
        composition.validate("owner").unwrap();
        let metrics = composition
            .recompute_metrics(&MathGlyphMetrics::default())
            .unwrap();
        assert_eq!(metrics.bounding_box.min_x, 0.0);
        assert_eq!(metrics.bounding_box.max_x, 250.0);
        assert_eq!(metrics.advance_width, 250.0);
    }

    #[test]
    fn rotation_changes_visual_bounds() {
        let mut rotated = layer("rotated", 0.0);
        rotated.transform.rotation_degrees = 90.0;
        let bounds = transformed_layer_bounds(&rotated);
        assert!((bounds.max_x - bounds.min_x - 200.0).abs() < 0.01);
        assert!((bounds.max_y - bounds.min_y - 100.0).abs() < 0.01);
    }

    #[test]
    fn self_reference_and_active_path_content_are_rejected() {
        let mut self_layer = layer("self", 0.0);
        self_layer.source = CompositionLayerSource::Symbol {
            symbol_id: "owner".to_string(),
            pack_id: None,
            metrics_snapshot: MathGlyphMetrics::default(),
        };
        let composition = MathGlyphComposition {
            layers: vec![self_layer],
            ..MathGlyphComposition::default()
        };
        assert!(composition.validate("owner").is_err());

        let primitive = DrawingPrimitive::Path {
            path_data: "M0,0<script>".to_string(),
            bounds: GlyphBoundingBox {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 10.0,
                max_y: 10.0,
            },
            stroke_width: 1.0,
            filled: false,
        };
        assert!(primitive.validate().is_err());
    }

    #[test]
    fn composition_roundtrips_with_versioned_layers() {
        let composition = MathGlyphComposition {
            layers: vec![layer("one", 25.0)],
            ..MathGlyphComposition::default()
        };
        let json = serde_json::to_string(&composition).unwrap();
        let restored: MathGlyphComposition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.schema_version, COMPOSITION_SCHEMA_VERSION);
        assert_eq!(restored.layers.len(), 1);
    }

    #[test]
    fn formula_layers_roundtrip_and_reject_unsafe_source() {
        let mut formula = layer("formula", 0.0);
        formula.source = CompositionLayerSource::Formula {
            latex: r"\overset{\star}{\longrightarrow}".to_string(),
            metrics_snapshot: MathGlyphMetrics::default(),
        };
        let composition = MathGlyphComposition {
            layers: vec![formula],
            ..MathGlyphComposition::default()
        };
        composition.validate("owner").unwrap();
        let json = serde_json::to_string(&composition).unwrap();
        let restored: MathGlyphComposition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, composition);

        let mut invalid = composition;
        if let CompositionLayerSource::Formula { latex, .. } = &mut invalid.layers[0].source {
            *latex = "x\u{0000}y".to_string();
        }
        assert!(invalid.validate("owner").is_err());
    }

    #[test]
    fn layer_color_is_portable_and_rejects_css_content() {
        let mut colored = layer("colored", 0.0);
        colored.color = Some("#7C3AED".to_string());
        let composition = MathGlyphComposition {
            layers: vec![colored],
            ..MathGlyphComposition::default()
        };
        composition.validate("owner").unwrap();

        let mut invalid = composition;
        invalid.layers[0].color = Some("url(https://example.invalid/x)".to_string());
        assert!(invalid.validate("owner").is_err());
    }
}
