use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const CONVERSION_PLAN_SCHEMA_VERSION: u16 = 1;
const MAX_FIDELITY: u16 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Formula,
    Drawing,
    CustomSymbol,
    Image,
    Table,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversionRoute {
    NativeOmml,
    OleFormula,
    NativeShapes,
    NativeTable,
    DrawingOle,
    Ooxml,
    Svg,
    Png,
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Latex,
    Mathml,
    Typst,
    Markdown,
}

impl ConversionRoute {
    fn priority(self) -> u8 {
        match self {
            Self::NativeOmml => 0,
            Self::OleFormula => 1,
            Self::NativeShapes => 2,
            Self::NativeTable => 3,
            Self::DrawingOle => 4,
            Self::Ooxml => 5,
            Self::Svg => 6,
            Self::Pdf => 7,
            Self::Png => 8,
            Self::Docx => 9,
            Self::Pptx => 10,
            Self::Xlsx => 11,
            Self::Mathml => 12,
            Self::Latex => 13,
            Self::Typst => 14,
            Self::Markdown => 15,
        }
    }

    fn supports(self, artifact: ArtifactKind) -> bool {
        match self {
            Self::NativeOmml | Self::OleFormula | Self::Mathml => artifact == ArtifactKind::Formula,
            Self::NativeShapes | Self::DrawingOle => {
                matches!(artifact, ArtifactKind::Drawing | ArtifactKind::CustomSymbol)
            }
            Self::NativeTable => artifact == ArtifactKind::Table,
            Self::Ooxml => true,
            Self::Svg | Self::Png | Self::Pdf => matches!(
                artifact,
                ArtifactKind::Formula
                    | ArtifactKind::Drawing
                    | ArtifactKind::CustomSymbol
                    | ArtifactKind::Image
            ),
            Self::Docx | Self::Pptx | Self::Xlsx => artifact == ArtifactKind::Document,
            Self::Latex | Self::Typst | Self::Markdown => matches!(
                artifact,
                ArtifactKind::Formula | ArtifactKind::Table | ArtifactKind::Document
            ),
        }
    }

    fn is_native(self) -> bool {
        matches!(
            self,
            Self::NativeOmml | Self::NativeShapes | Self::NativeTable
        )
    }

    fn is_vector(self) -> bool {
        matches!(
            self,
            Self::NativeOmml
                | Self::OleFormula
                | Self::NativeShapes
                | Self::NativeTable
                | Self::DrawingOle
                | Self::Ooxml
                | Self::Svg
                | Self::Pdf
        )
    }

    fn is_raster(self) -> bool {
        self == Self::Png
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LossDimension {
    Semantic,
    Visual,
    Editability,
    RoundTrip,
    Packaging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LossSeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionLoss {
    pub code: String,
    pub dimension: LossDimension,
    pub severity: LossSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteFidelity {
    pub semantic: u16,
    pub visual: u16,
    pub editability: u16,
    pub round_trip: u16,
}

impl RouteFidelity {
    pub const fn new(semantic: u16, visual: u16, editability: u16, round_trip: u16) -> Self {
        Self {
            semantic,
            visual,
            editability,
            round_trip,
        }
    }

    fn normalized(self) -> Self {
        Self {
            semantic: self.semantic.min(MAX_FIDELITY),
            visual: self.visual.min(MAX_FIDELITY),
            editability: self.editability.min(MAX_FIDELITY),
            round_trip: self.round_trip.min(MAX_FIDELITY),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteCapability {
    pub route: ConversionRoute,
    pub available: bool,
    pub fidelity: RouteFidelity,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub losses: Vec<ConversionLoss>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRequirements {
    #[serde(default)]
    pub minimum_semantic_fidelity: Option<u16>,
    #[serde(default)]
    pub minimum_visual_fidelity: Option<u16>,
    #[serde(default)]
    pub minimum_editability: Option<u16>,
    #[serde(default)]
    pub minimum_round_trip_fidelity: Option<u16>,
    #[serde(default)]
    pub prefer_native: bool,
    #[serde(default)]
    pub prefer_vector: bool,
    #[serde(default = "default_allow_raster")]
    pub allow_raster: bool,
}

const fn default_allow_raster() -> bool {
    true
}

impl Default for ConversionRequirements {
    fn default() -> Self {
        Self {
            minimum_semantic_fidelity: None,
            minimum_visual_fidelity: None,
            minimum_editability: None,
            minimum_round_trip_fidelity: None,
            prefer_native: false,
            prefer_vector: false,
            allow_raster: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRequest {
    pub artifact: ArtifactKind,
    pub host: String,
    pub platform: String,
    #[serde(default)]
    pub requirements: ConversionRequirements,
    pub capabilities: Vec<RouteCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionCandidate {
    pub route: ConversionRoute,
    pub score: u16,
    pub fidelity: RouteFidelity,
    pub eligible: bool,
    pub evidence: Vec<String>,
    pub losses: Vec<ConversionLoss>,
    pub rejection_reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversionPlanStatus {
    Planned,
    NoEligibleRoute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionPlan {
    pub schema_version: u16,
    pub status: ConversionPlanStatus,
    pub artifact: ArtifactKind,
    pub host: String,
    pub platform: String,
    pub selected: Option<ConversionCandidate>,
    pub fallbacks: Vec<ConversionCandidate>,
    pub rejected: Vec<ConversionCandidate>,
}

pub fn plan_conversion(request: &ConversionRequest) -> ConversionPlan {
    let mut unique = BTreeMap::<ConversionRoute, RouteCapability>::new();
    for capability in &request.capabilities {
        let replace = unique
            .get(&capability.route)
            .map(|current| capability_rank(capability) > capability_rank(current))
            .unwrap_or(true);
        if replace {
            unique.insert(capability.route, capability.clone());
        }
    }

    let mut candidates = unique
        .into_values()
        .map(|capability| evaluate_candidate(request, capability))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .eligible
            .cmp(&left.eligible)
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.route.priority().cmp(&right.route.priority()))
    });

    let mut eligible = candidates
        .iter()
        .filter(|candidate| candidate.eligible)
        .cloned();
    let selected = eligible.next();
    let fallbacks = eligible.collect();
    let rejected = candidates
        .into_iter()
        .filter(|candidate| !candidate.eligible)
        .collect();

    ConversionPlan {
        schema_version: CONVERSION_PLAN_SCHEMA_VERSION,
        status: if selected.is_some() {
            ConversionPlanStatus::Planned
        } else {
            ConversionPlanStatus::NoEligibleRoute
        },
        artifact: request.artifact,
        host: request.host.clone(),
        platform: request.platform.clone(),
        selected,
        fallbacks,
        rejected,
    }
}

fn capability_rank(capability: &RouteCapability) -> u32 {
    let fidelity = capability.fidelity.normalized();
    u32::from(capability.available) * 4_001
        + u32::from(fidelity.semantic)
        + u32::from(fidelity.visual)
        + u32::from(fidelity.editability)
        + u32::from(fidelity.round_trip)
}

fn evaluate_candidate(
    request: &ConversionRequest,
    capability: RouteCapability,
) -> ConversionCandidate {
    let fidelity = capability.fidelity.normalized();
    let mut rejection_reasons = Vec::new();
    if !capability.available {
        rejection_reasons.push("route is unavailable in the active host".to_string());
    }
    if !capability.route.supports(request.artifact) {
        rejection_reasons.push("route does not support this artifact kind".to_string());
    }
    if capability.route.is_raster() && !request.requirements.allow_raster {
        rejection_reasons.push("raster fallbacks are disabled".to_string());
    }
    require_minimum(
        &mut rejection_reasons,
        "semantic fidelity",
        fidelity.semantic,
        request.requirements.minimum_semantic_fidelity,
    );
    require_minimum(
        &mut rejection_reasons,
        "visual fidelity",
        fidelity.visual,
        request.requirements.minimum_visual_fidelity,
    );
    require_minimum(
        &mut rejection_reasons,
        "editability",
        fidelity.editability,
        request.requirements.minimum_editability,
    );
    require_minimum(
        &mut rejection_reasons,
        "round-trip fidelity",
        fidelity.round_trip,
        request.requirements.minimum_round_trip_fidelity,
    );

    let mut losses = capability.losses;
    add_derived_losses(&mut losses, capability.route, fidelity);
    deduplicate_losses(&mut losses);

    ConversionCandidate {
        route: capability.route,
        score: candidate_score(capability.route, fidelity, &request.requirements),
        fidelity,
        eligible: rejection_reasons.is_empty(),
        evidence: capability.evidence,
        losses,
        rejection_reasons,
    }
}

fn require_minimum(reasons: &mut Vec<String>, label: &str, actual: u16, required: Option<u16>) {
    if let Some(required) = required.map(|value| value.min(MAX_FIDELITY)) {
        if actual < required {
            reasons.push(format!("{label} {actual} is below required {required}"));
        }
    }
}

fn candidate_score(
    route: ConversionRoute,
    fidelity: RouteFidelity,
    requirements: &ConversionRequirements,
) -> u16 {
    let edit_weight = if requirements.minimum_editability.is_some() {
        5
    } else {
        2
    };
    let round_trip_weight = if requirements.minimum_round_trip_fidelity.is_some() {
        4
    } else {
        1
    };
    let visual_weight = if requirements.minimum_visual_fidelity.is_some() {
        4
    } else {
        3
    };
    let semantic_weight = if requirements.minimum_semantic_fidelity.is_some() {
        5
    } else {
        3
    };
    let total_weight = semantic_weight + visual_weight + edit_weight + round_trip_weight;
    let weighted = u32::from(fidelity.semantic) * semantic_weight
        + u32::from(fidelity.visual) * visual_weight
        + u32::from(fidelity.editability) * edit_weight
        + u32::from(fidelity.round_trip) * round_trip_weight;
    let mut score = (weighted / total_weight) as u16;
    if requirements.prefer_native && route.is_native() {
        score = score.saturating_add(25);
    }
    if requirements.prefer_vector && route.is_vector() {
        score = score.saturating_add(15);
    }
    score.min(MAX_FIDELITY)
}

fn add_derived_losses(
    losses: &mut Vec<ConversionLoss>,
    route: ConversionRoute,
    fidelity: RouteFidelity,
) {
    add_score_loss(
        losses,
        "semantic-degradation",
        LossDimension::Semantic,
        fidelity.semantic,
    );
    add_score_loss(
        losses,
        "visual-degradation",
        LossDimension::Visual,
        fidelity.visual,
    );
    add_score_loss(
        losses,
        "editability-loss",
        LossDimension::Editability,
        fidelity.editability,
    );
    add_score_loss(
        losses,
        "round-trip-loss",
        LossDimension::RoundTrip,
        fidelity.round_trip,
    );
    if route.is_raster() {
        losses.push(ConversionLoss {
            code: "rasterized-output".to_string(),
            dimension: LossDimension::Editability,
            severity: LossSeverity::High,
            message:
                "The selected route stores pixels instead of editable vector or semantic content."
                    .to_string(),
        });
    }
}

fn add_score_loss(
    losses: &mut Vec<ConversionLoss>,
    code: &str,
    dimension: LossDimension,
    score: u16,
) {
    if score >= MAX_FIDELITY {
        return;
    }
    let severity = if score >= 900 {
        LossSeverity::Low
    } else if score >= 700 {
        LossSeverity::Medium
    } else {
        LossSeverity::High
    };
    losses.push(ConversionLoss {
        code: code.to_string(),
        dimension,
        severity,
        message: format!("The route reports {score}/{MAX_FIDELITY} for {dimension:?}."),
    });
}

fn deduplicate_losses(losses: &mut Vec<ConversionLoss>) {
    let mut codes = BTreeSet::new();
    losses.retain(|loss| codes.insert(loss.code.clone()));
    losses.sort_by(|left, right| left.code.cmp(&right.code));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(route: ConversionRoute, fidelity: RouteFidelity) -> RouteCapability {
        RouteCapability {
            route,
            available: true,
            fidelity,
            evidence: Vec::new(),
            losses: Vec::new(),
        }
    }

    fn request(artifact: ArtifactKind, host: &str) -> ConversionRequest {
        ConversionRequest {
            artifact,
            host: host.to_string(),
            platform: "windows".to_string(),
            requirements: ConversionRequirements::default(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn word_formula_prefers_native_omml_for_editable_round_trip() {
        let mut request = request(ArtifactKind::Formula, "word");
        request.requirements.minimum_editability = Some(900);
        request.requirements.minimum_round_trip_fidelity = Some(850);
        request.requirements.prefer_native = true;
        request.capabilities = vec![
            capability(ConversionRoute::Png, RouteFidelity::new(0, 970, 0, 0)),
            capability(
                ConversionRoute::OleFormula,
                RouteFidelity::new(980, 980, 980, 950),
            ),
            capability(
                ConversionRoute::NativeOmml,
                RouteFidelity::new(1_000, 980, 1_000, 980),
            ),
        ];

        let plan = plan_conversion(&request);
        assert_eq!(plan.status, ConversionPlanStatus::Planned);
        assert_eq!(plan.selected.unwrap().route, ConversionRoute::NativeOmml);
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(plan.rejected[0].route, ConversionRoute::Png);
    }

    #[test]
    fn powerpoint_drawing_switches_between_native_and_svg_by_requirement() {
        let mut request = request(ArtifactKind::Drawing, "powerPoint");
        request.capabilities = vec![
            capability(
                ConversionRoute::NativeShapes,
                RouteFidelity::new(900, 920, 1_000, 950),
            ),
            capability(
                ConversionRoute::Svg,
                RouteFidelity::new(600, 1_000, 250, 300),
            ),
        ];
        request.requirements.minimum_editability = Some(900);
        request.requirements.prefer_native = true;
        assert_eq!(
            plan_conversion(&request).selected.unwrap().route,
            ConversionRoute::NativeShapes
        );

        request.requirements.minimum_editability = None;
        request.requirements.minimum_visual_fidelity = Some(990);
        request.requirements.prefer_native = false;
        request.requirements.prefer_vector = true;
        assert_eq!(
            plan_conversion(&request).selected.unwrap().route,
            ConversionRoute::Svg
        );
    }

    #[test]
    fn excel_formula_uses_ole_when_raster_is_forbidden() {
        let mut request = request(ArtifactKind::Formula, "excel");
        request.requirements.allow_raster = false;
        request.capabilities = vec![
            capability(
                ConversionRoute::OleFormula,
                RouteFidelity::new(970, 980, 980, 940),
            ),
            capability(ConversionRoute::Png, RouteFidelity::new(0, 990, 0, 0)),
        ];
        let plan = plan_conversion(&request);
        assert_eq!(plan.selected.unwrap().route, ConversionRoute::OleFormula);
        assert_eq!(plan.rejected[0].route, ConversionRoute::Png);
        assert!(plan.rejected[0]
            .rejection_reasons
            .iter()
            .any(|reason| reason.contains("raster")));
    }

    #[test]
    fn wps_table_prefers_native_table_over_source_fallback() {
        let mut request = request(ArtifactKind::Table, "wpsWriter");
        request.capabilities = vec![
            capability(
                ConversionRoute::Markdown,
                RouteFidelity::new(900, 600, 700, 650),
            ),
            capability(
                ConversionRoute::NativeTable,
                RouteFidelity::new(980, 950, 1_000, 940),
            ),
        ];
        let plan = plan_conversion(&request);
        assert_eq!(plan.status, ConversionPlanStatus::Planned);
        assert_eq!(plan.selected.unwrap().route, ConversionRoute::NativeTable);
    }

    #[test]
    fn unavailable_and_wrong_artifact_routes_fail_closed() {
        let mut request = request(ArtifactKind::Table, "wpsWriter");
        request.capabilities = vec![
            capability(
                ConversionRoute::NativeOmml,
                RouteFidelity::new(1_000, 1_000, 1_000, 1_000),
            ),
            RouteCapability {
                available: false,
                ..capability(
                    ConversionRoute::NativeTable,
                    RouteFidelity::new(1_000, 1_000, 1_000, 1_000),
                )
            },
        ];
        let plan = plan_conversion(&request);
        assert_eq!(plan.status, ConversionPlanStatus::NoEligibleRoute);
        assert!(plan.selected.is_none());
        assert_eq!(plan.rejected.len(), 2);
    }

    #[test]
    fn duplicate_capabilities_and_json_are_deterministic() {
        let mut request = request(ArtifactKind::Formula, "word");
        request.capabilities = vec![
            capability(ConversionRoute::Svg, RouteFidelity::new(500, 900, 100, 100)),
            capability(
                ConversionRoute::NativeOmml,
                RouteFidelity::new(1_000, 980, 1_000, 980),
            ),
            capability(ConversionRoute::Svg, RouteFidelity::new(600, 990, 200, 200)),
        ];
        let first = serde_json::to_string(&plan_conversion(&request)).unwrap();
        let second = serde_json::to_string(&plan_conversion(&request)).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("\"schemaVersion\":1"));
        assert_eq!(first.matches("\"route\":\"svg\"").count(), 1);
    }

    #[test]
    fn png_candidate_explains_raster_and_editability_loss() {
        let mut request = request(ArtifactKind::Drawing, "wpsPresentation");
        request.capabilities = vec![capability(
            ConversionRoute::Png,
            RouteFidelity::new(0, 970, 0, 0),
        )];
        let selected = plan_conversion(&request).selected.unwrap();
        assert!(selected
            .losses
            .iter()
            .any(|loss| loss.code == "rasterized-output"));
        assert!(selected
            .losses
            .iter()
            .any(|loss| loss.code == "editability-loss"));
    }
}
