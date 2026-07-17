use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use latexsnipper_ast::{
    AssetFormat, Document, DocumentVisitor, ExportFormat, FidelityClaim, FidelityDimensions,
    FidelityMeasurement, ImportOptions, InputFormat, TextCollector,
};
use latexsnipper_conversion::{DocumentExportService, DocumentImporter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const FIDELITY_REPORT_SCHEMA_VERSION: &str = "1.0.0";
pub const REQUIRED_DOCX_FEATURES: &[&str] = &[
    "runs",
    "styles",
    "headings",
    "lists",
    "tables",
    "merged-cells",
    "omml",
    "images",
    "headers",
    "footers",
    "notes",
    "sections",
    "comments-revisions",
    "opaque-parts",
];
pub const REQUIRED_PPTX_FEATURES: &[&str] = &[
    "slides",
    "layouts",
    "masters",
    "text-boxes",
    "shapes",
    "images",
];
pub const REQUIRED_XLSX_FEATURES: &[&str] = &[
    "cell-types",
    "formulas",
    "merged-cells",
    "styles",
    "dimensions",
    "tables",
    "charts",
    "conditional-formatting",
    "pivots",
    "macros",
    "embedded-objects",
];
pub const REQUIRED_PDF_FEATURES: &[&str] = &[
    "embedded-fonts",
    "missing-tounicode",
    "cjk",
    "rotated-text",
    "columns",
    "vector-graphics",
    "scanned-pages",
    "transparency",
    "annotations",
    "overlays",
];

#[derive(Debug, Error)]
pub enum FidelityError {
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid corpus: {0}")]
    InvalidCorpus(String),
    #[error("JSON failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("conversion failure: {0}")]
    Conversion(String),
    #[error("package failure: {0}")]
    Package(String),
}

pub type Result<T> = std::result::Result<T, FidelityError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FidelityFormat {
    Docx,
    Pptx,
    Xlsx,
    Pdf,
}

impl FidelityFormat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Docx => "DOCX",
            Self::Pptx => "PPTX",
            Self::Xlsx => "XLSX",
            Self::Pdf => "PDF",
        }
    }

    const fn input(self) -> InputFormat {
        match self {
            Self::Docx => InputFormat::OfficeDocx,
            Self::Pptx => InputFormat::OfficePptx,
            Self::Xlsx => InputFormat::OfficeXlsx,
            Self::Pdf => InputFormat::Pdf,
        }
    }

    const fn output(self) -> ExportFormat {
        match self {
            Self::Docx => ExportFormat::Docx,
            Self::Pptx => ExportFormat::Pptx,
            Self::Xlsx => ExportFormat::Xlsx,
            Self::Pdf => ExportFormat::Pdf,
        }
    }

    const fn required_features(self) -> &'static [&'static str] {
        match self {
            Self::Docx => REQUIRED_DOCX_FEATURES,
            Self::Pptx => REQUIRED_PPTX_FEATURES,
            Self::Xlsx => REQUIRED_XLSX_FEATURES,
            Self::Pdf => REQUIRED_PDF_FEATURES,
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Pptx => "pptx",
            Self::Xlsx => "xlsx",
            Self::Pdf => "pdf",
        }
    }

    const fn ooxml_contract(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Docx => Some((
                "word/document.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            )),
            Self::Pptx => Some((
                "ppt/presentation.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            )),
            Self::Xlsx => Some((
                "xl/workbook.xml",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
            )),
            Self::Pdf => None,
        }
    }
}

// ---------------------------------------------------------------------------
// OPC package structure validation
// ---------------------------------------------------------------------------

/// Validate that an OOXML ZIP package has the required structural elements:
/// `[Content_Types].xml`, `_rels/.rels`, the main document part, correct
/// content type declarations, and valid relationship graphs.
pub fn validate_ooxml_package_structure(bytes: &[u8], format: FidelityFormat) -> Result<()> {
    let Some((main_part, main_content_type)) = format.ooxml_contract() else {
        return Ok(());
    };

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| FidelityError::Package(format!("invalid OOXML ZIP package: {error}")))?;

    let mut names = BTreeSet::new();

    for index in 0..archive.len() {
        let name = {
            let file = archive.by_index(index).map_err(|error| {
                FidelityError::Package(format!("failed to inspect ZIP entry {index}: {error}"))
            })?;
            file.name().to_string()
        };
        validate_package_part_name(&name)?;
        names.insert(name);
    }

    for required in ["[Content_Types].xml", "_rels/.rels", main_part] {
        if !names.contains(required) {
            return Err(FidelityError::Package(format!(
                "{} package is missing required part '{}'",
                format.label(),
                required,
            )));
        }
    }

    let root_rels = read_package_text(&mut archive, "_rels/.rels")?;

    let office_document_type =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

    if !has_xml_element_with_attrs(
        &root_rels,
        "Relationship",
        &[("Type", office_document_type), ("Target", main_part)],
    ) {
        return Err(FidelityError::Package(format!(
            "{} package root relationships do not identify '{}'",
            format.label(),
            main_part,
        )));
    }

    let content_types = read_package_text(&mut archive, "[Content_Types].xml")?;

    let main_part_name = format!("/{main_part}");

    if !has_xml_element_with_attrs(
        &content_types,
        "Override",
        &[
            ("PartName", main_part_name.as_str()),
            ("ContentType", main_content_type),
        ],
    ) {
        return Err(FidelityError::Package(format!(
            "{} package does not declare the correct main content type for '{}'",
            format.label(),
            main_part,
        )));
    }

    // Every ZIP part must have a Content-Type.
    for part in &names {
        if part == "[Content_Types].xml" {
            continue;
        }

        let override_name = format!("/{part}");

        let has_override = has_xml_element_with_attrs(
            &content_types,
            "Override",
            &[("PartName", override_name.as_str())],
        );

        let has_default = part
            .rsplit_once('.')
            .map(|(_, extension)| {
                has_xml_element_with_attrs(&content_types, "Default", &[("Extension", extension)])
            })
            .unwrap_or(false);

        if !has_override && !has_default {
            return Err(FidelityError::Package(format!(
                "{} package part '{}' has no content type",
                format.label(),
                part,
            )));
        }
    }

    // All relationships must have Id / Type / Target, and internal targets must exist.
    let relationship_parts: Vec<String> = names
        .iter()
        .filter(|name| name.ends_with(".rels"))
        .cloned()
        .collect();

    for relationship_part in relationship_parts {
        let xml = read_package_text(&mut archive, &relationship_part)?;
        validate_relationship_part(&relationship_part, &xml, &names)?;
    }

    // PPTX-specific: verify critical parts have correct Content-Type overrides.
    if format == FidelityFormat::Pptx {
        let required_types: &[(&str, &str)] = &[
            (
                "ppt/presentation.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            ),
            (
                "ppt/presProps.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml",
            ),
            (
                "ppt/slideMasters/slideMaster1.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
            ),
            (
                "ppt/slideLayouts/slideLayout1.xml",
                "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
            ),
            (
                "ppt/theme/theme1.xml",
                "application/vnd.openxmlformats-officedocument.theme+xml",
            ),
        ];
        for (part, expected_type) in required_types {
            let part_name = format!("/{part}");
            if !has_xml_element_with_attrs(
                &content_types,
                "Override",
                &[
                    ("PartName", part_name.as_str()),
                    ("ContentType", expected_type),
                ],
            ) {
                return Err(FidelityError::Package(format!(
                    "PPTX part '{part}' does not declare required content type '{expected_type}'"
                )));
            }
        }

        // PPTX-specific: slideLayoutId must be >= 2147483648.
        let master_xml = read_package_text(&mut archive, "ppt/slideMasters/slideMaster1.xml")?;
        validate_pptx_slide_layout_ids(&master_xml)?;
    }

    Ok(())
}

fn validate_pptx_slide_layout_ids(master_xml: &str) -> Result<()> {
    for fragment in master_xml.split("<p:sldLayoutId ").skip(1) {
        let tag = fragment.split('>').next().unwrap_or(fragment);
        let id = xml_attribute(tag, "id")
            .ok_or_else(|| FidelityError::Package("PPTX slide layout id is missing".to_string()))?
            .parse::<u32>()
            .map_err(|_| FidelityError::Package("PPTX slide layout id is invalid".to_string()))?;
        if id < 2_147_483_648 {
            return Err(FidelityError::Package(format!(
                "PPTX slide layout id {id} is outside the PowerPoint-compatible range"
            )));
        }
    }
    Ok(())
}

fn read_package_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String> {
    let mut file = archive.by_name(name).map_err(|error| {
        FidelityError::Package(format!("failed to open package part '{name}': {error}"))
    })?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn validate_package_part_name(name: &str) -> Result<()> {
    if name.starts_with('/')
        || name.contains('\\')
        || Path::new(name)
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(FidelityError::Package(format!(
            "unsafe OOXML package part '{name}'"
        )));
    }
    Ok(())
}

fn xml_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let remainder = &tag[start..];
    let end = remainder.find('"')?;
    Some(&remainder[..end])
}

fn has_xml_element_with_attrs(xml: &str, element: &str, attrs: &[(&str, &str)]) -> bool {
    let needle = format!("<{element} ");
    xml.split(&needle).skip(1).any(|fragment| {
        let tag = fragment.split('>').next().unwrap_or(fragment);
        attrs
            .iter()
            .all(|(name, expected)| xml_attribute(tag, name) == Some(*expected))
    })
}

fn validate_relationship_part(
    relationship_part: &str,
    xml: &str,
    package_parts: &BTreeSet<String>,
) -> Result<()> {
    let base_dir = relationship_base_dir(relationship_part)?;

    for fragment in xml.split("<Relationship ").skip(1) {
        let tag = fragment.split('>').next().unwrap_or(fragment);

        let id = xml_attribute(tag, "Id").ok_or_else(|| {
            FidelityError::Package(format!(
                "{relationship_part} contains a relationship without Id"
            ))
        })?;

        let relationship_type = xml_attribute(tag, "Type").ok_or_else(|| {
            FidelityError::Package(format!(
                "{relationship_part} relationship '{id}' has no Type"
            ))
        })?;

        if relationship_type.is_empty() {
            return Err(FidelityError::Package(format!(
                "{relationship_part} relationship '{id}' has an empty Type"
            )));
        }

        let target = xml_attribute(tag, "Target").ok_or_else(|| {
            FidelityError::Package(format!(
                "{relationship_part} relationship '{id}' has no Target"
            ))
        })?;

        if xml_attribute(tag, "TargetMode") == Some("External") {
            continue;
        }

        let resolved = resolve_relationship_target(&base_dir, target)?;

        if !package_parts.contains(&resolved) {
            return Err(FidelityError::Package(format!(
                "{relationship_part} relationship '{id}' targets missing part '{resolved}'"
            )));
        }
    }

    Ok(())
}

fn relationship_base_dir(relationship_part: &str) -> Result<String> {
    if relationship_part == "_rels/.rels" {
        return Ok(String::new());
    }

    let marker = "/_rels/";

    let marker_index = relationship_part.rfind(marker).ok_or_else(|| {
        FidelityError::Package(format!(
            "invalid relationship part path '{relationship_part}'"
        ))
    })?;

    let prefix = &relationship_part[..marker_index];

    let relationship_file = &relationship_part[marker_index + marker.len()..];

    let source_file = relationship_file.strip_suffix(".rels").ok_or_else(|| {
        FidelityError::Package(format!("invalid relationship file '{relationship_part}'"))
    })?;

    let source_part = if prefix.is_empty() {
        source_file.to_string()
    } else {
        format!("{prefix}/{source_file}")
    };

    Ok(source_part
        .rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
        .unwrap_or_default())
}

fn resolve_relationship_target(base_dir: &str, target: &str) -> Result<String> {
    let combined = if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else if base_dir.is_empty() {
        target.to_string()
    } else {
        format!("{base_dir}/{target}")
    };

    let mut parts = Vec::new();

    for component in combined.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(FidelityError::Package(format!(
                        "relationship target escapes package root: '{target}'"
                    )));
                }
            }
            other => {
                if other.contains('\\') {
                    return Err(FidelityError::Package(format!(
                        "invalid relationship target '{target}'"
                    )));
                }
                parts.push(other);
            }
        }
    }

    Ok(parts.join("/"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorpusIndex {
    pub schema_version: String,
    pub cases: Vec<CorpusCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorpusCase {
    pub id: String,
    pub format: FidelityFormat,
    pub fixture: String,
    pub sha256: String,
    pub license: String,
    pub source: String,
    pub features: Vec<FeatureEvidence>,
    #[serde(default)]
    pub expected_text: Vec<String>,
    #[serde(default)]
    pub required_diagnostic_codes: Vec<String>,
    #[serde(default)]
    pub required_opaque_parts: Vec<String>,
    #[serde(default)]
    pub minimum_assets: usize,
    pub minimum_semantic_similarity: f64,
    pub minimum_layout_similarity: f64,
    #[serde(default)]
    pub visual_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeatureEvidence {
    pub feature: String,
    pub evidence_token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerKind {
    ReopenValidation,
    SemanticAstComparison,
    ExpectedDiagnosticComparison,
    AssetPreservation,
    OpaquePartPreservation,
    VisualRenderingComparison,
    ApplicationSpecificSmoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerResult {
    pub layer: LayerKind,
    pub status: LayerStatus,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseReport {
    pub id: String,
    pub format_pair: String,
    pub fixture_sha256: String,
    pub source: String,
    pub license: String,
    pub dimensions: FidelityDimensions,
    pub layers: Vec<LayerResult>,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FidelityReport {
    pub schema_version: String,
    pub source_commit: String,
    pub generated_at_utc: String,
    pub cases: Vec<CaseReport>,
    pub passed: bool,
    pub disclaimer: String,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub repository_root: PathBuf,
    pub source_commit: String,
    pub generated_at_utc: String,
    pub visual_artifact_dir: PathBuf,
}

impl RunOptions {
    pub fn ci(repository_root: PathBuf, source_commit: String, generated_at_utc: String) -> Self {
        Self {
            visual_artifact_dir: repository_root.join("target/fidelity/visual"),
            repository_root,
            source_commit,
            generated_at_utc,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisualArtifacts {
    pub candidate: PathBuf,
    pub additional_artifacts: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ApplicationSmokeResult {
    pub passed: bool,
    pub summary: String,
    pub evidence: Vec<String>,
    pub artifacts: Vec<PathBuf>,
}

/// Optional host integration for renderer and Office/viewer smoke tests.
/// Implementations run outside Core so they can use installed applications.
pub trait FidelityHarness {
    fn render(
        &self,
        case: &CorpusCase,
        source: &Path,
        converted: &Path,
        artifact_dir: &Path,
    ) -> Result<VisualArtifacts>;

    fn application_smoke(
        &self,
        case: &CorpusCase,
        source: &Path,
        converted: &Path,
        artifact_dir: &Path,
    ) -> Result<ApplicationSmokeResult>;
}

pub fn load_and_validate_index(path: &Path, repository_root: &Path) -> Result<CorpusIndex> {
    let bytes = fs::read(path)?;
    let index: CorpusIndex = serde_json::from_slice(&bytes)?;
    if index.schema_version != FIDELITY_REPORT_SCHEMA_VERSION {
        return Err(FidelityError::InvalidCorpus(format!(
            "unsupported schema version '{}'",
            index.schema_version
        )));
    }
    if index.cases.is_empty() {
        return Err(FidelityError::InvalidCorpus(
            "corpus has no cases".to_string(),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut formats = BTreeSet::new();
    for case in &index.cases {
        if !ids.insert(case.id.as_str()) {
            return Err(FidelityError::InvalidCorpus(format!(
                "duplicate case id '{}'",
                case.id
            )));
        }
        formats.insert(case.format.label());
        validate_case(case, repository_root)?;
    }
    for required in ["DOCX", "PPTX", "XLSX", "PDF"] {
        if !formats.contains(required) {
            return Err(FidelityError::InvalidCorpus(format!(
                "missing {required} corpus case"
            )));
        }
    }
    Ok(index)
}

fn validate_case(case: &CorpusCase, repository_root: &Path) -> Result<()> {
    if case.id.is_empty() || case.license.is_empty() || case.source.is_empty() {
        return Err(FidelityError::InvalidCorpus(format!(
            "case '{}' lacks identity, license, or source metadata",
            case.id
        )));
    }
    let feature_names = case
        .features
        .iter()
        .map(|feature| feature.feature.as_str())
        .collect::<BTreeSet<_>>();
    if feature_names.len() != case.features.len() {
        return Err(FidelityError::InvalidCorpus(format!(
            "case '{}' contains duplicate feature declarations",
            case.id
        )));
    }
    for (name, threshold) in [
        (
            "minimumSemanticSimilarity",
            case.minimum_semantic_similarity,
        ),
        ("minimumLayoutSimilarity", case.minimum_layout_similarity),
    ] {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(FidelityError::InvalidCorpus(format!(
                "case '{}' has invalid {name}={threshold}",
                case.id
            )));
        }
    }
    for required in case.format.required_features() {
        if !feature_names.contains(required) {
            return Err(FidelityError::InvalidCorpus(format!(
                "case '{}' lacks required feature '{}'",
                case.id, required
            )));
        }
    }
    let path = confined_path(repository_root, &case.fixture)?;
    let bytes = fs::read(&path)?;
    let actual = sha256(&bytes);
    if actual != case.sha256 {
        return Err(FidelityError::InvalidCorpus(format!(
            "case '{}' checksum mismatch: expected {}, got {}",
            case.id, case.sha256, actual
        )));
    }
    validate_ooxml_package_structure(&bytes, case.format)?;
    for feature in &case.features {
        if feature.evidence_token.is_empty()
            || !bytes
                .windows(feature.evidence_token.len())
                .any(|window| window == feature.evidence_token.as_bytes())
        {
            return Err(FidelityError::InvalidCorpus(format!(
                "case '{}' lacks byte evidence '{}' for feature '{}'",
                case.id, feature.evidence_token, feature.feature
            )));
        }
    }
    Ok(())
}

pub fn run(index: &CorpusIndex, options: &RunOptions) -> Result<FidelityReport> {
    run_with_harness(index, options, None)
}

pub fn run_with_harness(
    index: &CorpusIndex,
    options: &RunOptions,
    harness: Option<&dyn FidelityHarness>,
) -> Result<FidelityReport> {
    let mut cases = Vec::with_capacity(index.cases.len());
    for case in &index.cases {
        cases.push(run_case(case, options, harness)?);
    }
    let passed = cases.iter().all(|case| case.passed);
    Ok(FidelityReport {
        schema_version: FIDELITY_REPORT_SCHEMA_VERSION.to_string(),
        source_commit: options.source_commit.clone(),
        generated_at_utc: options.generated_at_utc.clone(),
        cases,
        passed,
        disclaimer: "Structural package validity is independent from visual parity. Skipped visual or application layers are not passes.".to_string(),
    })
}

fn run_case(
    case: &CorpusCase,
    options: &RunOptions,
    harness: Option<&dyn FidelityHarness>,
) -> Result<CaseReport> {
    let fixture = confined_path(&options.repository_root, &case.fixture)?;
    let bytes = fs::read(&fixture)?;
    validate_ooxml_package_structure(&bytes, case.format)?;
    let imported = DocumentImporter::from_bytes(
        &bytes,
        Some(case.format.input()),
        ImportOptions {
            preserve_unknown_parts: case.format != FidelityFormat::Pdf,
            preserve_layout: true,
            ..ImportOptions::default()
        },
    )
    .map_err(|error| FidelityError::Conversion(error.to_string()))?;
    let artifact = DocumentExportService::export(&imported, case.format.output())
        .map_err(|error| FidelityError::Conversion(error.to_string()))?;
    let exported = artifact
        .as_bytes()
        .ok_or_else(|| FidelityError::Conversion("export produced no bytes".to_string()))?;
    validate_ooxml_package_structure(exported, case.format)?;
    let reopened = DocumentImporter::from_bytes(
        exported,
        Some(case.format.input()),
        ImportOptions {
            preserve_unknown_parts: case.format != FidelityFormat::Pdf,
            preserve_layout: true,
            ..ImportOptions::default()
        },
    )
    .map_err(|error| FidelityError::Conversion(error.to_string()))?;

    let mut layers = Vec::new();
    layers.push(passed_layer(
        LayerKind::ReopenValidation,
        format!("{} output reopened successfully", case.format.label()),
        vec![format!("exportSha256={}", sha256(exported))],
    ));

    let semantic_score = semantic_similarity(&imported, &reopened);
    let layout_score = layout_similarity(&imported, &reopened);
    let semantic_pass = expected_text_present(case, &imported)
        && semantic_score >= case.minimum_semantic_similarity
        && layout_score >= case.minimum_layout_similarity;
    layers.push(layer(
        LayerKind::SemanticAstComparison,
        semantic_pass,
        format!(
            "semantic={semantic_score:.6} (required>={:.6}), layout={layout_score:.6} (required>={:.6})",
            case.minimum_semantic_similarity, case.minimum_layout_similarity
        ),
        vec![
            format!("semanticScore={semantic_score:.6}"),
            format!("layoutScore={layout_score:.6}"),
        ],
    ));

    let actual_diagnostics = imported
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<BTreeSet<_>>();
    let missing_diagnostics = case
        .required_diagnostic_codes
        .iter()
        .filter(|code| !actual_diagnostics.contains(code.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    layers.push(layer(
        LayerKind::ExpectedDiagnosticComparison,
        missing_diagnostics.is_empty(),
        if missing_diagnostics.is_empty() {
            "all required diagnostics were emitted".to_string()
        } else {
            format!("missing diagnostics: {}", missing_diagnostics.join(", "))
        },
        imported
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect(),
    ));

    let source_assets = semantic_assets(&imported);
    let reopened_assets = semantic_assets(&reopened);
    let asset_pass = source_assets >= case.minimum_assets && reopened_assets >= source_assets;
    layers.push(layer(
        LayerKind::AssetPreservation,
        asset_pass,
        format!("sourceAssets={source_assets}, reopenedAssets={reopened_assets}"),
        Vec::new(),
    ));

    let missing_parts = if case.format == FidelityFormat::Pdf {
        Vec::new()
    } else {
        changed_opaque_parts(&bytes, exported, &case.required_opaque_parts)?
    };
    layers.push(layer(
        LayerKind::OpaquePartPreservation,
        missing_parts.is_empty(),
        if case.format == FidelityFormat::Pdf {
            "not applicable to PDF".to_string()
        } else if missing_parts.is_empty() {
            "all required opaque parts survived byte-for-byte".to_string()
        } else {
            format!(
                "missing or changed opaque parts: {}",
                missing_parts.join(", ")
            )
        },
        case.required_opaque_parts.clone(),
    ));

    layers.push(run_visual_layer(case, options, harness, &fixture, exported));
    layers.push(run_application_layer(
        case, options, harness, &fixture, exported,
    ));

    let passed = layers
        .iter()
        .all(|result| result.status != LayerStatus::Failed);
    let visual = layers
        .iter()
        .find(|result| result.layer == LayerKind::VisualRenderingComparison)
        .map(|result| result.status)
        .unwrap_or(LayerStatus::Skipped);
    let app_smoke = layers
        .iter()
        .find(|result| result.layer == LayerKind::ApplicationSpecificSmoke)
        .map(|result| result.status)
        .unwrap_or(LayerStatus::Skipped);

    Ok(CaseReport {
        id: case.id.clone(),
        format_pair: format!("{} -> {}", case.format.label(), case.format.label()),
        fixture_sha256: case.sha256.clone(),
        source: case.source.clone(),
        license: case.license.clone(),
        dimensions: FidelityDimensions {
            structural_validity: scored_measurement(
                FidelityClaim::Verified,
                1.0,
                "reopen validation passed",
            ),
            semantic_preservation: scored_measurement(
                if semantic_score == 1.0 {
                    FidelityClaim::Verified
                } else {
                    FidelityClaim::Partial
                },
                semantic_score,
                "semantic AST snapshot comparison",
            ),
            layout_preservation: FidelityMeasurement {
                claim: FidelityClaim::Partial,
                score: Some(layout_score),
                evidence: vec![
                    "page size, page count, and geometry coverage comparison".to_string()
                ],
                limitations: vec!["application layout engine was not executed".to_string()],
            },
            visual_fidelity: FidelityMeasurement {
                claim: if visual == LayerStatus::Passed {
                    FidelityClaim::Verified
                } else {
                    FidelityClaim::NotMeasured
                },
                score: None,
                evidence: Vec::new(),
                limitations: vec![
                    "visual layer was skipped unless an external harness ran".to_string()
                ],
            },
            editability: FidelityMeasurement {
                claim: if case.format == FidelityFormat::Pdf {
                    FidelityClaim::Unsupported
                } else {
                    FidelityClaim::Partial
                },
                score: None,
                evidence: vec!["reopened semantic AST node coverage".to_string()],
                limitations: vec!["opaque objects are not semantically editable".to_string()],
            },
            round_trip_fidelity: FidelityMeasurement {
                claim: if case.format == FidelityFormat::Pdf {
                    FidelityClaim::Unsupported
                } else {
                    FidelityClaim::Partial
                },
                score: (case.format != FidelityFormat::Pdf).then_some(semantic_score),
                evidence: vec!["source -> AST -> same-format -> AST".to_string()],
                limitations: vec![if app_smoke == LayerStatus::Passed {
                    "application smoke passed".to_string()
                } else {
                    "application-specific smoke was not executed".to_string()
                }],
            },
        },
        layers,
        passed,
    })
}

fn run_visual_layer(
    case: &CorpusCase,
    options: &RunOptions,
    harness: Option<&dyn FidelityHarness>,
    fixture: &Path,
    exported: &[u8],
) -> LayerResult {
    let Some(reference) = case.visual_reference.as_deref() else {
        return skipped_layer(
            LayerKind::VisualRenderingComparison,
            "no golden rendering is declared; visual fidelity remains not measured",
            vec![options.visual_artifact_dir.to_string_lossy().into_owned()],
        );
    };
    let Some(harness) = harness else {
        return skipped_layer(
            LayerKind::VisualRenderingComparison,
            "golden rendering exists but no renderer harness is available",
            vec![options.visual_artifact_dir.to_string_lossy().into_owned()],
        );
    };
    let reference = match confined_path(&options.repository_root, reference) {
        Ok(path) => path,
        Err(error) => {
            return failed_layer(
                LayerKind::VisualRenderingComparison,
                error.to_string(),
                vec![options.visual_artifact_dir.to_string_lossy().into_owned()],
            );
        }
    };
    let artifact_dir = options.visual_artifact_dir.join(&case.id);
    if let Err(error) = fs::create_dir_all(&artifact_dir) {
        return failed_layer(
            LayerKind::VisualRenderingComparison,
            error.to_string(),
            vec![artifact_dir.to_string_lossy().into_owned()],
        );
    }
    let converted = artifact_dir.join(format!("converted.{}", case.format.extension()));
    if let Err(error) = fs::write(&converted, exported) {
        return failed_layer(
            LayerKind::VisualRenderingComparison,
            error.to_string(),
            vec![artifact_dir.to_string_lossy().into_owned()],
        );
    }
    match harness.render(case, fixture, &converted, &artifact_dir) {
        Ok(artifacts) => {
            let mut paths = vec![
                reference.to_string_lossy().into_owned(),
                artifacts.candidate.to_string_lossy().into_owned(),
            ];
            paths.extend(
                artifacts
                    .additional_artifacts
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned()),
            );
            match compare_rendered_images(&reference, &artifacts.candidate) {
                Ok(score) => LayerResult {
                    layer: LayerKind::VisualRenderingComparison,
                    status: if score >= 0.99 {
                        LayerStatus::Passed
                    } else {
                        LayerStatus::Failed
                    },
                    summary: format!("RGBA similarity={score:.6}, required>=0.990000"),
                    evidence: vec![format!("score={score:.6}")],
                    artifacts: paths,
                },
                Err(error) => failed_layer(
                    LayerKind::VisualRenderingComparison,
                    error.to_string(),
                    paths,
                ),
            }
        }
        Err(error) => failed_layer(
            LayerKind::VisualRenderingComparison,
            error.to_string(),
            vec![artifact_dir.to_string_lossy().into_owned()],
        ),
    }
}

fn run_application_layer(
    case: &CorpusCase,
    options: &RunOptions,
    harness: Option<&dyn FidelityHarness>,
    fixture: &Path,
    exported: &[u8],
) -> LayerResult {
    let Some(harness) = harness else {
        return skipped_layer(
            LayerKind::ApplicationSpecificSmoke,
            "Microsoft Office or equivalent application harness is not available",
            Vec::new(),
        );
    };
    let artifact_dir = options
        .visual_artifact_dir
        .join(&case.id)
        .join("application");
    if let Err(error) = fs::create_dir_all(&artifact_dir) {
        return failed_layer(
            LayerKind::ApplicationSpecificSmoke,
            error.to_string(),
            vec![artifact_dir.to_string_lossy().into_owned()],
        );
    }
    let converted = artifact_dir.join(format!("converted.{}", case.format.extension()));
    if let Err(error) = fs::write(&converted, exported) {
        return failed_layer(
            LayerKind::ApplicationSpecificSmoke,
            error.to_string(),
            vec![artifact_dir.to_string_lossy().into_owned()],
        );
    }
    match harness.application_smoke(case, fixture, &converted, &artifact_dir) {
        Ok(result) => LayerResult {
            layer: LayerKind::ApplicationSpecificSmoke,
            status: if result.passed {
                LayerStatus::Passed
            } else {
                LayerStatus::Failed
            },
            summary: result.summary,
            evidence: result.evidence,
            artifacts: result
                .artifacts
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        },
        Err(error) => failed_layer(
            LayerKind::ApplicationSpecificSmoke,
            error.to_string(),
            vec![artifact_dir.to_string_lossy().into_owned()],
        ),
    }
}

pub fn compare_rendered_images(reference: &Path, candidate: &Path) -> Result<f64> {
    let reference = image::open(reference)
        .map_err(|error| FidelityError::InvalidCorpus(error.to_string()))?
        .to_rgba8();
    let candidate = image::open(candidate)
        .map_err(|error| FidelityError::InvalidCorpus(error.to_string()))?
        .to_rgba8();
    if reference.dimensions() != candidate.dimensions() {
        return Ok(0.0);
    }
    let total_error = reference
        .as_raw()
        .iter()
        .zip(candidate.as_raw())
        .map(|(left, right)| (*left as f64 - *right as f64).abs() / 255.0)
        .sum::<f64>();
    Ok(1.0 - total_error / reference.as_raw().len().max(1) as f64)
}

fn semantic_similarity(source: &Document, reopened: &Document) -> f64 {
    let source_snapshot = semantic_snapshot(source);
    let reopened_snapshot = semantic_snapshot(reopened);
    let page_score = ratio(source_snapshot.pages, reopened_snapshot.pages);
    let block_score = ratio(source_snapshot.blocks, reopened_snapshot.blocks);
    let type_score =
        multiset_similarity(&source_snapshot.block_types, &reopened_snapshot.block_types);
    let text_score = token_similarity(&source_snapshot.text, &reopened_snapshot.text);
    (page_score + block_score + type_score + text_score) / 4.0
}

fn layout_similarity(source: &Document, reopened: &Document) -> f64 {
    let page_count = ratio(source.pages.len(), reopened.pages.len());
    let paired_pages = source.pages.iter().zip(&reopened.pages).collect::<Vec<_>>();
    let page_size = if paired_pages.is_empty() {
        1.0
    } else {
        paired_pages
            .iter()
            .map(|(left, right)| {
                (dimension_similarity(left.width, right.width)
                    + dimension_similarity(left.height, right.height))
                    / 2.0
            })
            .sum::<f64>()
            / paired_pages.len() as f64
    };
    let source_geometry = source
        .all_blocks()
        .iter()
        .filter(|block| block.source().and_then(|source| source.region).is_some())
        .count();
    let reopened_geometry = reopened
        .all_blocks()
        .iter()
        .filter(|block| block.source().and_then(|source| source.region).is_some())
        .count();
    let geometry_coverage = ratio(source_geometry, reopened_geometry);
    (page_count + page_size + geometry_coverage) / 3.0
}

fn dimension_similarity(left: f32, right: f32) -> f64 {
    if left == 0.0 && right == 0.0 {
        return 1.0;
    }
    if left <= 0.0 || right <= 0.0 {
        return 0.0;
    }
    f64::from(left.min(right) / left.max(right))
}

#[derive(Debug)]
struct SemanticSnapshot {
    pages: usize,
    blocks: usize,
    block_types: BTreeMap<String, usize>,
    text: String,
}

fn semantic_snapshot(document: &Document) -> SemanticSnapshot {
    let mut block_types = BTreeMap::new();
    for block in document.all_blocks() {
        *block_types
            .entry(block.type_name().to_string())
            .or_insert(0) += 1;
    }
    let mut collector = TextCollector::new();
    collector.visit_document(document);
    SemanticSnapshot {
        pages: document.pages.len(),
        blocks: document.block_count(),
        block_types,
        text: collector.text,
    }
}

fn expected_text_present(case: &CorpusCase, document: &Document) -> bool {
    let mut collector = TextCollector::new();
    collector.visit_document(document);
    case.expected_text
        .iter()
        .all(|expected| collector.text.contains(expected))
}

fn semantic_assets(document: &Document) -> usize {
    document
        .assets
        .iter()
        .filter(|asset| asset.format != AssetFormat::OoxmlPart)
        .count()
}

fn changed_opaque_parts(
    source: &[u8],
    exported: &[u8],
    required: &[String],
) -> Result<Vec<String>> {
    let mut source_archive = zip::ZipArchive::new(Cursor::new(source))
        .map_err(|error| FidelityError::Package(error.to_string()))?;
    let mut exported_archive = zip::ZipArchive::new(Cursor::new(exported))
        .map_err(|error| FidelityError::Package(error.to_string()))?;
    let mut changed = Vec::new();
    for expected in required {
        let mut source_part = match source_archive.by_name(expected) {
            Ok(part) => part,
            Err(_) => {
                changed.push(expected.clone());
                continue;
            }
        };
        let mut source_bytes = Vec::new();
        source_part.read_to_end(&mut source_bytes)?;
        drop(source_part);
        let mut exported_part = match exported_archive.by_name(expected) {
            Ok(part) => part,
            Err(_) => {
                changed.push(expected.clone());
                continue;
            }
        };
        let mut exported_bytes = Vec::new();
        exported_part.read_to_end(&mut exported_bytes)?;
        if source_bytes != exported_bytes {
            changed.push(expected.clone());
        }
    }
    Ok(changed)
}

fn ratio(left: usize, right: usize) -> f64 {
    match (left, right) {
        (0, 0) => 1.0,
        (0, _) | (_, 0) => 0.0,
        _ => left.min(right) as f64 / left.max(right) as f64,
    }
}

fn multiset_similarity(left: &BTreeMap<String, usize>, right: &BTreeMap<String, usize>) -> f64 {
    let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
    let intersection = keys
        .iter()
        .map(|key| {
            left.get(*key)
                .unwrap_or(&0)
                .min(right.get(*key).unwrap_or(&0))
        })
        .sum::<usize>();
    let union = keys
        .iter()
        .map(|key| {
            left.get(*key)
                .unwrap_or(&0)
                .max(right.get(*key).unwrap_or(&0))
        })
        .sum::<usize>();
    ratio(intersection, union)
}

fn token_similarity(left: &str, right: &str) -> f64 {
    let left = left.split_whitespace().collect::<BTreeSet<_>>();
    let right = right.split_whitespace().collect::<BTreeSet<_>>();
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    ratio(intersection, union)
}

fn scored_measurement(claim: FidelityClaim, score: f64, evidence: &str) -> FidelityMeasurement {
    FidelityMeasurement {
        claim,
        score: Some(score),
        evidence: vec![evidence.to_string()],
        limitations: Vec::new(),
    }
}

fn layer(kind: LayerKind, passed: bool, summary: String, evidence: Vec<String>) -> LayerResult {
    LayerResult {
        layer: kind,
        status: if passed {
            LayerStatus::Passed
        } else {
            LayerStatus::Failed
        },
        summary,
        evidence,
        artifacts: Vec::new(),
    }
}

fn passed_layer(kind: LayerKind, summary: String, evidence: Vec<String>) -> LayerResult {
    layer(kind, true, summary, evidence)
}

fn skipped_layer(kind: LayerKind, summary: &str, artifacts: Vec<String>) -> LayerResult {
    LayerResult {
        layer: kind,
        status: LayerStatus::Skipped,
        summary: summary.to_string(),
        evidence: Vec::new(),
        artifacts,
    }
}

fn failed_layer(kind: LayerKind, summary: String, artifacts: Vec<String>) -> LayerResult {
    LayerResult {
        layer: kind,
        status: LayerStatus::Failed,
        summary,
        evidence: Vec::new(),
        artifacts,
    }
}

fn confined_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || relative.contains(':')
        || relative.contains('\\')
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(FidelityError::InvalidCorpus(format!(
            "fixture path '{relative}' is not portable and confined"
        )));
    }
    let canonical_root = fs::canonicalize(root)?;
    let canonical_path = fs::canonicalize(root.join(path))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(FidelityError::InvalidCorpus(format!(
            "fixture path '{relative}' escapes the repository through a link"
        )));
    }
    Ok(canonical_path)
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_has_the_required_feature_contract() {
        assert_eq!(REQUIRED_DOCX_FEATURES.len(), 14);
        assert_eq!(REQUIRED_PPTX_FEATURES.len(), 6);
        assert_eq!(REQUIRED_XLSX_FEATURES.len(), 11);
        assert_eq!(REQUIRED_PDF_FEATURES.len(), 10);
    }

    #[test]
    fn portable_path_confinement_rejects_cross_platform_escape_forms() {
        let root = std::env::temp_dir().join(format!(
            "latexsnipper_fidelity_paths_{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("fidelity/fixtures")).unwrap();
        fs::write(root.join("fidelity/fixtures/test.pdf"), b"fixture").unwrap();
        for path in ["../escape", "/absolute", "C:/escape", "C:\\escape"] {
            assert!(confined_path(&root, path).is_err(), "accepted {path}");
        }
        assert_eq!(
            confined_path(&root, "fidelity/fixtures/test.pdf").unwrap(),
            fs::canonicalize(root.join("fidelity/fixtures/test.pdf")).unwrap()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn similarity_is_bounded_and_symmetric() {
        let left = BTreeMap::from([("paragraph".to_string(), 2), ("table".to_string(), 1)]);
        let right = BTreeMap::from([("paragraph".to_string(), 1)]);
        let first = multiset_similarity(&left, &right);
        let second = multiset_similarity(&right, &left);
        assert!((0.0..=1.0).contains(&first));
        assert_eq!(first, second);
    }

    #[test]
    fn generated_pptx_fixture_exercises_asset_import() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fidelity/fixtures/presentation-rich.pptx");
        let bytes = fs::read(path).unwrap();
        let document = DocumentImporter::from_bytes(
            &bytes,
            Some(InputFormat::OfficePptx),
            ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(semantic_assets(&document), 1);
    }

    #[test]
    fn visual_similarity_distinguishes_identical_and_changed_pixels() {
        let root = std::env::temp_dir().join(format!(
            "latexsnipper_fidelity_images_{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let reference = root.join("reference.png");
        let identical = root.join("identical.png");
        let changed = root.join("changed.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]))
            .save(&reference)
            .unwrap();
        fs::copy(&reference, &identical).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
            .save(&changed)
            .unwrap();
        assert_eq!(
            compare_rendered_images(&reference, &identical).unwrap(),
            1.0
        );
        assert!(compare_rendered_images(&reference, &changed).unwrap() < 0.3);
        fs::remove_dir_all(root).ok();
    }
}
