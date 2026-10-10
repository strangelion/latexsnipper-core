//! Runnable trusted-Rust format and scoped command-extension examples.
use latexsnipper_conversion::{
    conversion_registry::*, latex_ast::LatexNode, CapabilityTarget, DocumentConverter,
    FormulaConversionMode, FormulaInputFormat, OutputFormat,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Power {
    variable: String,
    exponent: u8,
}

fn rejected(message: impl Into<String>) -> ConversionRegistryError {
    ConversionRegistryError::new(ConversionRegistryErrorCode::InputRejected, message)
}
fn parse_power(source: &str) -> Result<Power, ConversionRegistryError> {
    let power: Power = serde_json::from_str(source).map_err(|error| rejected(error.to_string()))?;
    if power.variable.len() != 1
        || !power.variable.as_bytes()[0].is_ascii_lowercase()
        || power.exponent > 9
    {
        return Err(rejected(
            "demo format supports one ASCII variable and exponent 0..9",
        ));
    }
    Ok(power)
}
fn power_ast(variable: String, exponent: u8) -> LatexNode {
    LatexNode::Superscript {
        base: Box::new(LatexNode::Text(variable)),
        exp: Box::new(LatexNode::Text(exponent.to_string())),
    }
}
fn to_omml(node: LatexNode) -> Result<String, ConversionRegistryError> {
    DocumentConverter::convert_formula_string(
        &node.to_string(),
        FormulaInputFormat::Latex,
        OutputFormat::OMML,
        FormulaConversionMode::Strict,
    )
    .map_err(|error| rejected(error.to_string()))
}

struct PowerBackend;
impl RegisteredFormulaBackend for PowerBackend {
    fn check_ready(&self) -> Result<(), ConversionRegistryError> {
        Ok(())
    }
    fn convert(
        &self,
        request: &RegisteredConversionRequest,
    ) -> Result<ConversionCandidate, ConversionRegistryError> {
        let power = parse_power(&request.content)?;
        let ast = power_ast(power.variable, power.exponent);
        let content = if request.output == "latex" {
            ast.to_string()
        } else {
            to_omml(ast)?
        };
        Ok(ConversionCandidate { content, losses: vec![ConversionLoss {
            code: "FINITE_POWER_FORMAT".into(), message: "Only a single power is represented; source spelling is reconstructed, not preserved".into(),
        }] })
    }
}

// A deliberately finite syntax addition, not string-replacement hot patching.
struct SquareCommandBackend;
impl RegisteredFormulaBackend for SquareCommandBackend {
    fn check_ready(&self) -> Result<(), ConversionRegistryError> {
        Ok(())
    }
    fn convert(
        &self,
        request: &RegisteredConversionRequest,
    ) -> Result<ConversionCandidate, ConversionRegistryError> {
        let variable = request
            .content
            .strip_prefix(r"\demoSquare{")
            .and_then(|rest| rest.strip_suffix('}'))
            .filter(|variable| variable.len() == 1 && variable.as_bytes()[0].is_ascii_lowercase())
            .ok_or_else(|| {
                rejected("only the complete demoSquare{one ASCII letter} expression is supported")
            })?;
        let content = to_omml(power_ast(variable.into(), 2))?;
        Ok(ConversionCandidate { content, losses: vec![ConversionLoss {
            code: "CUSTOM_COMMAND_RECONSTRUCTED".into(), message: "Finite demo command expanded through AST; arbitrary macros and trailing content are refused".into(),
        }] })
    }
}

fn definition(id: &str, input: &str, outputs: &[&str]) -> ConversionBackendDescriptor {
    ConversionBackendDescriptor {
        schema_version: CONVERSION_REGISTRY_VERSION,
        id: id.into(),
        version: "1.0.0".into(),
        configuration_sha256: format!("{:x}", Sha256::digest(id.as_bytes())),
        license: "MIT OR Apache-2.0".into(),
        limitations: vec!["Trusted local demonstration only; not installed or sandboxed".into()],
        routes: outputs
            .iter()
            .map(|output| ConversionBackendRoute {
                input: input.into(),
                output: (*output).into(),
                mode: FormulaConversionMode::BestEffort,
                target: CapabilityTarget::Native,
            })
            .collect(),
        max_input_bytes: 1024,
        max_output_bytes: 4096,
    }
}
fn request(input: &str, output: &str, backend: &str, content: &str) -> RegisteredConversionRequest {
    RegisteredConversionRequest {
        schema_version: CONVERSION_REGISTRY_VERSION,
        input: input.into(),
        output: output.into(),
        backend: Some(backend.into()),
        content: content.into(),
        mode: FormulaConversionMode::BestEffort,
        // Production callers must hash actual Core build, definitions, fonts and styles.
        context_sha256: format!("{:x}", Sha256::digest(b"isolated-example-context-v1")),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = SemanticConversionRegistry::default();
    registry.register_trusted_format(
        ConversionFormatDescriptor {
            schema_version: CONVERSION_REGISTRY_VERSION,
            id: "demo:power-json".into(),
            mime_type: "application/json".into(),
            description: "One ASCII variable raised to exponent 0..9".into(),
        },
        |source| parse_power(source).map(|_| ()),
    )?;
    registry.register_trusted_backend(
        definition("demo:power", "demo:power-json", &["latex", "omml"]),
        PowerBackend,
    )?;
    registry.register_trusted_backend(
        definition("demo:square-command", "latex", &["omml"]),
        SquareCommandBackend,
    )?;
    let power = request(
        "demo:power-json",
        "omml",
        "demo:power",
        r#"{"variable":"x","exponent":3}"#,
    );
    assert_eq!(
        registry.convert(&power).unwrap_err().code,
        ConversionRegistryErrorCode::BackendDisabled
    );
    registry.set_backend_enabled("demo:power", true)?;
    registry.set_backend_enabled("demo:square-command", true)?;
    let power_result = DocumentConverter::convert_registered_formula(&registry, &power)?;
    let mut altered = power.clone();
    altered.content = r#"{"variable":"y","exponent":4}"#.into();
    let other_result = registry.convert(&altered)?;
    assert_ne!(
        power_result.candidate.content,
        other_result.candidate.content
    );
    assert_ne!(power_result.route_sha256, other_result.route_sha256);
    let square = request("latex", "omml", "demo:square-command", r"\demoSquare{z}");
    let square_result = registry.convert(&square)?;
    assert_eq!(
        square_result.candidate.content,
        to_omml(power_ast("z".into(), 2))?
    );
    let mut unsupported = square.clone();
    unsupported.content.push_str("+y");
    assert_eq!(
        registry.convert(&unsupported).unwrap_err().code,
        ConversionRegistryErrorCode::InputRejected
    );
    let mut implicit = square;
    implicit.backend = None;
    assert_ne!(
        registry.convert(&implicit)?.candidate.content,
        square_result.candidate.content
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&[power_result, other_result, square_result])?
    );
    Ok(())
}
