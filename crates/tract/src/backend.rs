use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_runtime::{AccelerationMode, InferenceSession, ModelHandle, RuntimeBackend};
use prost::Message;
use tract_onnx::prelude::*;

use crate::session::TractSession;

/// A `RuntimeBackend` implementation using the `tract` pure-Rust ONNX runtime.
///
/// Unlike `OnnxRuntimeBackend`, this does not depend on the ONNX Runtime C++
/// library and can compile to `wasm32-unknown-unknown`.
pub struct TractBackend {
    models_dir: Option<PathBuf>,
    session_cache: Mutex<HashMap<String, std::sync::Arc<TractSession>>>,
}

impl TractBackend {
    /// Create a new TractBackend.
    ///
    /// - `Some(path)`: load models from filesystem (native)
    /// - `None`: use byte-based loading only (WASM)
    pub fn new(models_dir: Option<PathBuf>) -> Self {
        Self {
            models_dir,
            session_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Validate that an ONNX artifact is a decodable model protobuf.
    ///
    /// Graph import and optimization are intentionally deferred until the
    /// package resolver can attach configuration such as a concrete input
    /// shape. Importing a large detector during artifact registration would
    /// otherwise block the browser worker before the package is complete.
    pub fn validate_model_bytes(model_bytes: &[u8]) -> Result<()> {
        let mut proto = tract_onnx::pb::ModelProto::decode(model_bytes).map_err(|error| {
            SnipperError::Runtime(format!("Tract model decode failed: {error}"))
        })?;
        normalize_symbolic_dimensions(&mut proto, None);
        Ok(())
    }
}

impl RuntimeBackend for TractBackend {
    fn clear_sessions(&self) {
        match self.session_cache.lock() {
            Ok(mut cache) => {
                cache.clear();
                log::info!("Tract session cache cleared");
            }
            Err(error) => {
                log::error!("Failed to clear Tract session cache: {}", error);
            }
        }
    }

    fn create_session(
        &self,
        handle: &ModelHandle,
        _acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        let cache_key = handle.id().to_string();

        // Check cache first
        {
            let cache = self.session_cache.lock().map_err(|e| {
                SnipperError::Runtime(format!("Session cache lock poisoned: {}", e))
            })?;
            if let Some(session) = cache.get(&cache_key) {
                return Ok(Box::new(TractSession::clone(session)));
            }
        }

        // Load model bytes
        let model_bytes = if let Some(bytes) = handle.model_bytes() {
            bytes.to_vec()
        } else if let Some(path) = handle.model_path() {
            std::fs::read(path)
                .map_err(|e| SnipperError::Runtime(format!("Failed to read model file: {}", e)))?
        } else if let (Some(models_dir), category, variant) =
            (&self.models_dir, handle.category(), handle.variant())
        {
            // Try to find model by category/variant
            let candidates = [
                models_dir.join(category).join(variant).join("model.onnx"),
                models_dir
                    .join(category)
                    .join(variant)
                    .join(format!("{}.onnx", category)),
                models_dir
                    .join(category)
                    .join(variant)
                    .join("model_int8.onnx"),
            ];
            let mut found = None;
            for path in &candidates {
                if path.exists() {
                    found = Some(std::fs::read(path).map_err(|e| {
                        SnipperError::Runtime(format!("Failed to read model: {}", e))
                    })?);
                    break;
                }
            }
            found.ok_or_else(|| {
                SnipperError::Runtime(format!("Model not found for {}/{}", category, variant))
            })?
        } else {
            return Err(SnipperError::Runtime("No model source available".into()));
        };

        // Decode first so exporter-generated symbolic dimensions can be
        // normalized without changing concrete model shapes.
        let mut proto = tract_onnx::pb::ModelProto::decode(&*model_bytes)
            .map_err(|e| SnipperError::Runtime(format!("Tract model decode failed: {e}")))?;
        let normalized_dimensions = normalize_symbolic_dimensions(&mut proto, handle.input_shape());
        if normalized_dimensions > 0 {
            log::warn!("Normalized {normalized_dimensions} ONNX symbolic dimension(s) for Tract");
        }
        let model = onnx()
            .model_for_proto_model(&proto)
            .map_err(|e| SnipperError::Runtime(format!("Tract model load failed: {e:#}")))?;

        let model = model
            .into_optimized()
            .map_err(|e| SnipperError::Runtime(format!("Tract model optimize failed: {e:#}")))?;

        let model = model
            .into_runnable()
            .map_err(|e| SnipperError::Runtime(format!("Tract model compile failed: {e:#}")))?;

        let session = std::sync::Arc::new(TractSession::new(model));

        // Cache it
        {
            let mut cache = self.session_cache.lock().map_err(|e| {
                SnipperError::Runtime(format!("Session cache lock poisoned: {}", e))
            })?;
            cache.insert(cache_key, session.clone());
        }

        Ok(Box::new(TractSession::clone(&session)))
    }

    fn name(&self) -> &str {
        "tract"
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn normalize_symbolic_dimensions(
    model: &mut tract_onnx::pb::ModelProto,
    static_input_shape: Option<&[usize]>,
) -> usize {
    model.graph.as_mut().map_or(0, |graph| {
        normalize_graph_dimensions(graph, static_input_shape)
    })
}

fn normalize_graph_dimensions(
    graph: &mut tract_onnx::pb::GraphProto,
    static_input_shape: Option<&[usize]>,
) -> usize {
    let mut normalized = 0;
    for value in graph
        .input
        .iter_mut()
        .chain(graph.output.iter_mut())
        .chain(graph.value_info.iter_mut())
    {
        if let Some(value_type) = value.r#type.as_mut() {
            normalized += normalize_type_dimensions(value_type, static_input_shape);
        }
    }
    for node in &mut graph.node {
        for attribute in &mut node.attribute {
            if let Some(nested) = attribute.g.as_mut() {
                normalized += normalize_graph_dimensions(nested, static_input_shape);
            }
            for nested in &mut attribute.graphs {
                normalized += normalize_graph_dimensions(nested, static_input_shape);
            }
            for value_type in &mut attribute.type_protos {
                normalized += normalize_type_dimensions(value_type, static_input_shape);
            }
        }
    }
    normalized
}

fn normalize_type_dimensions(
    value_type: &mut tract_onnx::pb::TypeProto,
    static_input_shape: Option<&[usize]>,
) -> usize {
    use tract_onnx::pb::tensor_shape_proto::dimension::Value as DimensionValue;
    use tract_onnx::pb::type_proto::Value as TypeValue;

    let Some(TypeValue::TensorType(tensor)) = value_type.value.as_mut() else {
        return 0;
    };
    let Some(shape) = tensor.shape.as_mut() else {
        return 0;
    };
    let mut normalized = 0;
    for dimension in &mut shape.dim {
        let Some(DimensionValue::DimParam(parameter)) = dimension.value.as_mut() else {
            continue;
        };
        if let Some(normalized_parameter) =
            normalize_dimension_parameter(parameter, static_input_shape)
        {
            *parameter = normalized_parameter;
            normalized += 1;
        }
    }
    normalized
}

fn normalize_dimension_parameter(
    parameter: &str,
    static_input_shape: Option<&[usize]>,
) -> Option<String> {
    if static_input_shape.is_none() {
        if parameter == "batch_size" {
            return Some("1".to_string());
        }
        if parameter == "num_channels" {
            return Some("3".to_string());
        }
    }
    let mut translated = parameter.to_string();
    for axis in ["height", "width"] {
        for divisor in [2, 4, 8, 16, 32] {
            let pattern = format!("((({axis} - 1)//{divisor})) + 1");
            let replacement = format!("(({axis}+{})/{divisor})", divisor - 1);
            translated = translated.replace(&pattern, &replacement);
        }
    }
    translated.retain(|character| !character.is_ascii_whitespace());
    for axis in ["height", "width"] {
        let mut floor_pyramid = format!("floor({axis}/2-1/2)");
        for depth in 1..=16u32 {
            let divisor = 1u32 << depth;
            let pattern = format!("{floor_pyramid}+1");
            let replacement = format!("(({axis}+{})/{divisor})", divisor - 1);
            translated = translated.replace(&pattern, &replacement);
            floor_pyramid = format!("floor({floor_pyramid}/2)");
        }
    }
    translated = parenthesize_top_level_chain(&translated, '+');
    if let Some([batch, channels, height, width]) = static_input_shape {
        translated = translated
            .replace("height", &height.to_string())
            .replace("width", &width.to_string());
        translated = match translated.as_str() {
            "batch" | "batch_size" => batch.to_string(),
            "channels" | "num_channels" => channels.to_string(),
            _ => translated,
        };
    }
    // Tract's TDim grammar accepts arithmetic expressions, but not exporter
    // helper calls such as `floor(...)`. Treat those annotations as stable
    // symbolic names instead of letting the ONNX loader try to parse them as
    // executable dimension expressions. The graph operators still determine
    // the concrete tensor shapes at runtime.
    let tract_expression = !translated.contains("//")
        && !translated.contains("floor(")
        && translated.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '_' | '+' | '-' | '*' | '/' | '(' | ')')
        });
    if tract_expression {
        return (translated != parameter).then_some(translated);
    }

    let mut symbol: String = parameter
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if symbol
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        symbol.insert_str(0, "dim_");
    }
    (symbol != parameter).then_some(symbol)
}

fn parenthesize_top_level_chain(expression: &str, operator: char) -> String {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (index, character) in expression.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if character == operator && depth == 0 => {
                parts.push(&expression[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if parts.len() < 2 {
        return expression.to_string();
    }
    parts.push(&expression[start..]);
    let mut nested = parts.pop().unwrap().to_string();
    while let Some(part) = parts.pop() {
        nested = format!("({part}{operator}{nested})");
    }
    nested
}

#[cfg(test)]
mod tests {
    use super::*;
    use tract_onnx::pb::tensor_shape_proto::dimension::Value as DimensionValue;
    use tract_onnx::pb::type_proto::{Tensor, Value as TypeValue};
    use tract_onnx::pb::{TensorShapeProto, TypeProto, ValueInfoProto};

    #[test]
    fn normalizes_dot_in_symbolic_dimension_without_changing_concrete_shape() {
        let mut model = tract_onnx::pb::ModelProto::default();
        let graph = model.graph.get_or_insert_default();
        graph.input.push(ValueInfoProto {
            name: "x".to_string(),
            r#type: Some(TypeProto {
                denotation: String::new(),
                value: Some(TypeValue::TensorType(Tensor {
                    elem_type: 1,
                    shape: Some(TensorShapeProto {
                        dim: vec![
                            tract_onnx::pb::tensor_shape_proto::Dimension {
                                denotation: String::new(),
                                value: Some(DimensionValue::DimParam(
                                    "DynamicDimension.0".to_string(),
                                )),
                            },
                            tract_onnx::pb::tensor_shape_proto::Dimension {
                                denotation: String::new(),
                                value: Some(DimensionValue::DimValue(3)),
                            },
                        ],
                    }),
                })),
            }),
            doc_string: String::new(),
        });

        assert_eq!(normalize_symbolic_dimensions(&mut model, None), 1);
        let graph = model.graph.unwrap();
        let shape = match graph.input[0]
            .r#type
            .as_ref()
            .and_then(|value| value.value.as_ref())
            .unwrap()
        {
            TypeValue::TensorType(tensor) => tensor.shape.as_ref().unwrap(),
        };
        assert_eq!(
            shape.dim[0].value,
            Some(DimensionValue::DimParam("DynamicDimension_0".to_string()))
        );
        assert_eq!(shape.dim[1].value, Some(DimensionValue::DimValue(3)));
    }

    #[test]
    fn normalizes_exporter_dimension_expressions_to_tract_symbols() {
        let mut shape = TypeProto {
            denotation: String::new(),
            value: Some(TypeValue::TensorType(Tensor {
                elem_type: 1,
                shape: Some(TensorShapeProto {
                    dim: vec![tract_onnx::pb::tensor_shape_proto::Dimension {
                        denotation: String::new(),
                        value: Some(DimensionValue::DimParam(
                            "(((height - 1)//2)) + 1".to_string(),
                        )),
                    }],
                }),
            })),
        };
        assert_eq!(normalize_type_dimensions(&mut shape, None), 1);
        let TypeValue::TensorType(tensor) = shape.value.unwrap();
        let value = tensor.shape.unwrap().dim[0].value.clone().unwrap();
        let DimensionValue::DimParam(parameter) = value else {
            panic!("expected symbolic dimension");
        };
        assert!(parameter.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '_' | '+' | '-' | '*' | '/' | '(' | ')')
        }));
        assert!(!parameter.contains("//"));
    }

    #[test]
    fn fixes_exported_rgb_channel_dimension() {
        assert_eq!(
            normalize_dimension_parameter("batch_size", None),
            Some("1".to_string())
        );
        assert_eq!(
            normalize_dimension_parameter("num_channels", None),
            Some("3".to_string())
        );
    }

    #[test]
    fn converts_floor_dimension_annotations_to_stable_symbols() {
        let parameter = "floor(height/2 - 1/2) + 1";
        let normalized = normalize_dimension_parameter(parameter, None).unwrap();
        assert_eq!(normalized, "((height+1)/2)");
        assert!(normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric()
                || matches!(character, '_' | '+' | '-' | '*' | '/' | '(' | ')')));

        let nested = "(floor(floor(floor(height/2 - 1/2)/2)/2) + 1)*(floor(floor(floor(width/2 - 1/2)/2)/2) + 1)";
        let normalized = normalize_dimension_parameter(nested, None).unwrap();
        assert_eq!(normalized, "(((height+7)/8))*(((width+7)/8))");
        assert!(!normalized.contains("floor"));

        let three_scales = format!("{nested}+{nested}+{nested}");
        let normalized = normalize_dimension_parameter(&three_scales, None).unwrap();
        assert_eq!(normalized.matches('+').count(), 8);
        assert!(normalized.starts_with('('));
        assert!(normalized.ends_with(')'));
    }

    #[test]
    fn specializes_dimensions_from_package_metadata_without_category_assumptions() {
        let shape = [1, 3, 768, 768];
        assert_eq!(
            normalize_dimension_parameter("height", Some(&shape)),
            Some("768".to_string())
        );
        assert_eq!(
            normalize_dimension_parameter("batch", Some(&shape)),
            Some("1".to_string())
        );
        assert_eq!(
            normalize_dimension_parameter("floor(floor(height/2 - 1/2)/2) + 1", Some(&shape)),
            Some("((768+3)/4)".to_string())
        );
        let replacement_shape = [2, 4, 640, 960];
        assert_eq!(
            normalize_dimension_parameter("batch", Some(&replacement_shape)),
            Some("2".to_string())
        );
        assert_eq!(
            normalize_dimension_parameter("width", Some(&replacement_shape)),
            Some("960".to_string())
        );
    }
}
