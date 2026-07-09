use latexsnipper_ast::{Inline, Rect, ShapeBlock, ShapeStyle, ShapeType, TextRun};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse an SVG string and extract shapes as AST `ShapeBlock` items.
///
/// Handles basic SVG primitives: `<rect>`, `<circle>`, `<ellipse>`, `<line>`,
/// `<polyline>`, `<polygon>`, `<text>`, and `<g>` (groups).
///
/// Unsupported elements are silently skipped.
pub fn parse_svg_to_shapes(svg: &str) -> Vec<ShapeBlock> {
    let mut reader = Reader::from_str(svg);
    reader.config_mut().trim_text(true);
    let mut shapes = Vec::new();
    let mut buf = Vec::new();
    let mut current_transform: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let attrs = collect_attrs(e);

                match tag.as_str() {
                    "g" => {
                        if let Some(t) = attrs.get("transform") {
                            current_transform = Some(t.clone());
                        }
                    }
                    "rect" => {
                        if let Some(shape) = parse_rect(&attrs, &current_transform) {
                            shapes.push(shape);
                        }
                    }
                    "circle" | "ellipse" => {
                        if let Some(shape) = parse_circle_ellipse(&tag, &attrs, &current_transform)
                        {
                            shapes.push(shape);
                        }
                    }
                    "line" => {
                        if let Some(shape) = parse_line(&attrs, &current_transform) {
                            shapes.push(shape);
                        }
                    }
                    "polyline" | "polygon" => {
                        if let Some(shape) = parse_poly(&attrs, &current_transform) {
                            shapes.push(shape);
                        }
                    }
                    "text" => {
                        let text_content = read_text_content(&mut reader, &mut buf);
                        if !text_content.is_empty() {
                            shapes.push(ShapeBlock {
                                shape_type: ShapeType::Custom,
                                text: vec![Inline::Text(TextRun::new(text_content))],
                                geometry: parse_geometry(&attrs, &current_transform),
                                style: parse_style(&attrs),
                                source: None,
                                transform: None,
                                layer: None,
                                accessibility: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let attrs = collect_attrs(e);

                match tag.as_str() {
                    "g" => {
                        // Self-closing <g/> — just clear transform
                        if attrs.contains_key("transform") {
                            current_transform = None;
                        }
                    }
                    "rect" => {
                        if let Some(shape) = parse_rect(&attrs, &current_transform) {
                            shapes.push(shape);
                        }
                    }
                    "circle" | "ellipse" => {
                        if let Some(shape) = parse_circle_ellipse(&tag, &attrs, &current_transform)
                        {
                            shapes.push(shape);
                        }
                    }
                    "line" => {
                        if let Some(shape) = parse_line(&attrs, &current_transform) {
                            shapes.push(shape);
                        }
                    }
                    "polyline" | "polygon" => {
                        if let Some(shape) = parse_poly(&attrs, &current_transform) {
                            shapes.push(shape);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "g" {
                    current_transform = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                log::warn!("SVG parse error: {}", e);
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    shapes
}

// ── Element parsers ──

fn parse_rect(
    attrs: &std::collections::HashMap<String, String>,
    transform: &Option<String>,
) -> Option<ShapeBlock> {
    let x = attrs
        .get("x")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let y = attrs
        .get("y")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let w = attrs.get("width").and_then(|v| v.parse::<f32>().ok())?;
    let h = attrs.get("height").and_then(|v| v.parse::<f32>().ok())?;

    Some(ShapeBlock {
        shape_type: ShapeType::Rectangle,
        text: Vec::new(),
        geometry: Some(apply_transform(Rect::new(x, y, w, h), transform)),
        style: parse_style(attrs),
        source: None,
        transform: None,
        layer: None,
        accessibility: None,
    })
}

fn parse_circle_ellipse(
    tag: &str,
    attrs: &std::collections::HashMap<String, String>,
    transform: &Option<String>,
) -> Option<ShapeBlock> {
    let cx = attrs
        .get("cx")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let cy = attrs
        .get("cy")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let (w, h) = if tag == "circle" {
        let r = attrs.get("r").and_then(|v| v.parse::<f32>().ok())?;
        (r * 2.0, r * 2.0)
    } else {
        let rx = attrs
            .get("rx")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        let ry = attrs
            .get("ry")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        (rx * 2.0, ry * 2.0)
    };

    Some(ShapeBlock {
        shape_type: ShapeType::Ellipse,
        text: Vec::new(),
        geometry: Some(apply_transform(
            Rect::new(cx - w / 2.0, cy - h / 2.0, w, h),
            transform,
        )),
        style: parse_style(attrs),
        source: None,
        transform: None,
        layer: None,
        accessibility: None,
    })
}

fn parse_line(
    attrs: &std::collections::HashMap<String, String>,
    transform: &Option<String>,
) -> Option<ShapeBlock> {
    let x1 = attrs
        .get("x1")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let y1 = attrs
        .get("y1")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let x2 = attrs
        .get("x2")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let y2 = attrs
        .get("y2")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);

    Some(ShapeBlock {
        shape_type: ShapeType::Line,
        text: Vec::new(),
        geometry: Some(apply_transform(
            Rect::new(x1.min(x2), y1.min(y2), (x1 - x2).abs(), (y1 - y2).abs()),
            transform,
        )),
        style: parse_style(attrs),
        source: None,
        transform: None,
        layer: None,
        accessibility: None,
    })
}

fn parse_poly(
    attrs: &std::collections::HashMap<String, String>,
    transform: &Option<String>,
) -> Option<ShapeBlock> {
    let points_str = attrs.get("points")?;
    let coords: Vec<f32> = points_str
        .split([' ', ',', '\n', '\t'])
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();

    if coords.len() < 4 {
        return None;
    }

    let min_x = coords.iter().step_by(2).cloned().fold(f32::MAX, f32::min);
    let max_x = coords.iter().step_by(2).cloned().fold(f32::MIN, f32::max);
    let min_y = coords
        .iter()
        .skip(1)
        .step_by(2)
        .cloned()
        .fold(f32::MAX, f32::min);
    let max_y = coords
        .iter()
        .skip(1)
        .step_by(2)
        .cloned()
        .fold(f32::MIN, f32::max);

    Some(ShapeBlock {
        shape_type: ShapeType::Custom,
        text: Vec::new(),
        geometry: Some(apply_transform(
            Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
            transform,
        )),
        style: parse_style(attrs),
        source: None,
        transform: None,
        layer: None,
        accessibility: None,
    })
}

// ── Helpers ──

fn collect_attrs(e: &quick_xml::events::BytesStart) -> std::collections::HashMap<String, String> {
    let mut attrs = std::collections::HashMap::new();
    for attr in e.attributes().flatten() {
        if let Ok(key) = String::from_utf8(attr.key.as_ref().to_vec()) {
            if let Ok(val) = String::from_utf8(attr.value.to_vec()) {
                attrs.insert(key, val);
            }
        }
    }
    attrs
}

fn parse_geometry(
    attrs: &std::collections::HashMap<String, String>,
    transform: &Option<String>,
) -> Option<Rect> {
    let x = attrs
        .get("x")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let y = attrs
        .get("y")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.0);
    let w = attrs
        .get("width")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(100.0);
    let h = attrs
        .get("height")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(20.0);
    Some(apply_transform(Rect::new(x, y, w, h), transform))
}

fn parse_style(attrs: &std::collections::HashMap<String, String>) -> Option<ShapeStyle> {
    let fill = attrs.get("fill").or(attrs.get("fill-opacity"));
    let stroke = attrs.get("stroke");
    let stroke_width = attrs
        .get("stroke-width")
        .and_then(|v| v.parse::<f32>().ok());

    if fill.is_none() && stroke.is_none() && stroke_width.is_none() {
        return None;
    }

    Some(ShapeStyle {
        fill_color: fill.and_then(|v| parse_color_str(v)),
        stroke_color: stroke.and_then(|v| parse_color_str(v)),
        stroke_width,
        opacity: attrs.get("opacity").and_then(|v| v.parse::<f32>().ok()),
    })
}

fn parse_color_str(value: &str) -> Option<latexsnipper_ast::Color> {
    let value = value.trim();
    if value.starts_with('#') || value.starts_with("rgb") || value.len() <= 20 {
        Some(latexsnipper_ast::Color {
            value: value.to_string(),
            alpha: None,
        })
    } else {
        None
    }
}

fn apply_transform(rect: Rect, transform: &Option<String>) -> Rect {
    let Some(transform_str) = transform else {
        return rect;
    };

    // Simple translate handling: translate(tx, ty)
    if let Some(rest) = transform_str.strip_prefix("translate(") {
        if let Some(end) = rest.find(')') {
            let parts: Vec<f32> = rest[..end]
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if parts.len() >= 2 {
                return Rect::new(
                    rect.x + parts[0],
                    rect.y + parts[1],
                    rect.width,
                    rect.height,
                );
            }
        }
    }

    rect
}

fn read_text_content(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> String {
    let mut text = String::new();
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Text(ref e)) => {
                if let Ok(t) = e.unescape() {
                    text.push_str(&t);
                }
            }
            Ok(Event::End(_)) | Ok(Event::Empty(_)) => break,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_svg_rect() {
        let svg = r#"<svg><rect x="10" y="20" width="100" height="50" fill="red"/></svg>"#;
        let shapes = parse_svg_to_shapes(svg);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].shape_type, ShapeType::Rectangle);
        let geom = shapes[0].geometry.unwrap();
        assert_eq!(geom.x, 10.0);
        assert_eq!(geom.y, 20.0);
        assert_eq!(geom.width, 100.0);
        assert_eq!(geom.height, 50.0);
    }

    #[test]
    fn parse_svg_circle() {
        let svg = r#"<svg><circle cx="50" cy="50" r="40" fill="blue"/></svg>"#;
        let shapes = parse_svg_to_shapes(svg);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].shape_type, ShapeType::Ellipse);
    }

    #[test]
    fn parse_svg_line() {
        let svg = r#"<svg><line x1="10" y1="20" x2="100" y2="80" stroke="black"/></svg>"#;
        let shapes = parse_svg_to_shapes(svg);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].shape_type, ShapeType::Line);
    }

    #[test]
    fn parse_svg_text() {
        let svg = r#"<svg><text x="10" y="20">Hello World</text></svg>"#;
        let shapes = parse_svg_to_shapes(svg);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].shape_type, ShapeType::Custom);
        assert!(!shapes[0].text.is_empty());
    }

    #[test]
    fn parse_svg_group() {
        let svg = r#"<svg><g transform="translate(50,50)">
            <rect x="0" y="0" width="30" height="20"/>
            <circle cx="15" cy="10" r="5"/>
        </g></svg>"#;
        let shapes = parse_svg_to_shapes(svg);
        assert_eq!(shapes.len(), 2);
        assert!(
            shapes[0].geometry.unwrap().x > 40.0,
            "rect should be translated by group"
        );
    }

    #[test]
    fn parse_empty_svg() {
        let shapes = parse_svg_to_shapes("<svg></svg>");
        assert!(shapes.is_empty());
    }

    #[test]
    fn parse_svg_style() {
        let svg = r##"<svg><rect width="50" height="30" fill="#ff0000" stroke="#00ff00" stroke-width="2"/></svg>"##;
        let shapes = parse_svg_to_shapes(svg);
        assert_eq!(shapes.len(), 1);
        let style = shapes[0].style.as_ref().unwrap();
        assert!(style.fill_color.is_some());
        assert!(style.stroke_color.is_some());
        assert_eq!(style.stroke_width, Some(2.0));
    }
}
