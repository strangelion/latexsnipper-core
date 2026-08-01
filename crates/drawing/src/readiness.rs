use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    DrawingAdapterCapabilities, DrawingOutputFormat, DrawingPackageProfile, DrawingSecurityPolicy,
    DrawingSourceLanguage, SourcePreservingAdapter,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DrawingValidationLevel {
    Declared,
    ParserAvailable,
    CompilerDetected,
    PackageSetValidated,
    SmokeCompilePassed,
    GoldenRenderPassed,
    ProductionRecommended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingAdapterReadiness {
    pub language: DrawingSourceLanguage,
    pub level: DrawingValidationLevel,
    pub capabilities: DrawingAdapterCapabilities,
    pub experimental: bool,
    pub blocked: bool,
    pub requires_setup: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingCompilerReadiness {
    pub id: String,
    pub detected: bool,
    pub executable_sha256: Option<String>,
    pub version: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingPackageReadiness {
    pub profile: DrawingPackageProfile,
    pub allowed: bool,
    pub package_lock_sha256: Option<String>,
    pub validated: bool,
    pub experimental: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingOutputReadiness {
    pub format: DrawingOutputFormat,
    pub available: bool,
    pub office_default: bool,
    pub export_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingReadiness {
    pub schema_version: u32,
    pub adapters: Vec<DrawingAdapterReadiness>,
    pub compilers: Vec<DrawingCompilerReadiness>,
    pub package_profiles: Vec<DrawingPackageReadiness>,
    pub output_formats: Vec<DrawingOutputReadiness>,
    pub security_policy_ready: bool,
}

pub fn drawing_readiness(
    policy: &DrawingSecurityPolicy,
    package_locks: &BTreeMap<DrawingPackageProfile, String>,
) -> DrawingReadiness {
    let languages = [
        DrawingSourceLanguage::Tikz,
        DrawingSourceLanguage::SvgSource,
        DrawingSourceLanguage::DrawingJson,
        DrawingSourceLanguage::Mermaid,
        DrawingSourceLanguage::GraphvizDot,
        DrawingSourceLanguage::PlantUml,
        DrawingSourceLanguage::Asymptote,
        DrawingSourceLanguage::MetaPost,
        DrawingSourceLanguage::Pstricks,
    ];
    let mut adapters = languages
        .into_iter()
        .map(|language| {
            use crate::DrawingSourceAdapter;
            let adapter = SourcePreservingAdapter::for_language(language);
            let capabilities = adapter.capabilities();
            let allowed = policy.allowed_source_languages.contains(&language);
            let blocked = language == DrawingSourceLanguage::Pstricks || !allowed;
            let experimental = matches!(
                language,
                DrawingSourceLanguage::PlantUml
                    | DrawingSourceLanguage::Asymptote
                    | DrawingSourceLanguage::MetaPost
                    | DrawingSourceLanguage::Pstricks
            );
            let compiler_key = compiler_key(language);
            let compiler_detected = compiler_key.is_none_or(|key| {
                policy
                    .allowed_executables
                    .get(key)
                    .is_some_and(crate::ExecutableIdentity::is_strong)
            });
            let level = if capabilities.structured_parse && allowed {
                if compiler_detected {
                    DrawingValidationLevel::CompilerDetected
                } else {
                    DrawingValidationLevel::ParserAvailable
                }
            } else if compiler_detected && allowed {
                DrawingValidationLevel::CompilerDetected
            } else {
                DrawingValidationLevel::Declared
            };
            DrawingAdapterReadiness {
                language,
                level,
                capabilities,
                experimental,
                blocked,
                requires_setup: allowed && !compiler_detected,
                diagnostic: blocked
                    .then(|| "adapter is disabled by the active security policy".to_owned())
                    .or_else(|| {
                        (allowed && !compiler_detected).then(|| {
                            "compiler identity is not pinned; smoke and golden evidence are notRun"
                                .to_owned()
                        })
                    }),
            }
        })
        .collect::<Vec<_>>();
    adapters.sort_by_key(|adapter| adapter.language);

    let mut compilers = [
        "tectonic",
        "mermaid",
        "graphviz",
        "plantuml",
        "asymptote",
        "system-tex",
    ]
    .into_iter()
    .map(|id| {
        let identity = policy.allowed_executables.get(id);
        DrawingCompilerReadiness {
            id: id.to_owned(),
            detected: identity.is_some_and(crate::ExecutableIdentity::is_strong),
            executable_sha256: identity.map(|identity| identity.sha256.clone()),
            version: identity.map(|identity| identity.version.clone()),
            diagnostic: identity
                .is_none()
                .then(|| "not configured; compile contracts are notRun".to_owned()),
        }
    })
    .collect::<Vec<_>>();
    compilers.sort_by(|left, right| left.id.cmp(&right.id));

    let profiles = [
        DrawingPackageProfile::BaseTikz,
        DrawingPackageProfile::PgfPlots,
        DrawingPackageProfile::CircuitTikz,
        DrawingPackageProfile::TikzCd,
        DrawingPackageProfile::Forest,
        DrawingPackageProfile::ChemFig,
    ];
    let package_profiles = profiles
        .into_iter()
        .map(|profile| {
            let lock = package_locks
                .get(&profile)
                .filter(|digest| valid_sha256(digest));
            DrawingPackageReadiness {
                profile,
                allowed: policy.allowed_package_profiles.contains(&profile),
                package_lock_sha256: lock.cloned(),
                validated: lock.is_some(),
                experimental: profile == DrawingPackageProfile::ChemFig,
            }
        })
        .collect();

    DrawingReadiness {
        schema_version: 1,
        adapters,
        compilers,
        package_profiles,
        output_formats: vec![
            DrawingOutputReadiness {
                format: DrawingOutputFormat::Svg,
                available: true,
                office_default: true,
                export_only: false,
            },
            DrawingOutputReadiness {
                format: DrawingOutputFormat::Png,
                available: true,
                office_default: false,
                export_only: false,
            },
            DrawingOutputReadiness {
                format: DrawingOutputFormat::Pdf,
                available: true,
                office_default: false,
                export_only: true,
            },
            DrawingOutputReadiness {
                format: DrawingOutputFormat::WebP,
                available: false,
                office_default: false,
                export_only: false,
            },
            DrawingOutputReadiness {
                format: DrawingOutputFormat::Eps,
                available: false,
                office_default: false,
                export_only: true,
            },
        ],
        security_policy_ready: !policy.allow_shell_escape
            && !policy.allow_network
            && !policy.allow_absolute_paths
            && !policy.allow_parent_path,
    }
}

fn compiler_key(language: DrawingSourceLanguage) -> Option<&'static str> {
    match language {
        DrawingSourceLanguage::Tikz => Some("tectonic"),
        DrawingSourceLanguage::Mermaid => Some("mermaid"),
        DrawingSourceLanguage::GraphvizDot => Some("graphviz"),
        DrawingSourceLanguage::PlantUml => Some("plantuml"),
        DrawingSourceLanguage::Asymptote => Some("asymptote"),
        DrawingSourceLanguage::MetaPost | DrawingSourceLanguage::Pstricks => Some("system-tex"),
        DrawingSourceLanguage::SvgSource | DrawingSourceLanguage::DrawingJson => None,
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_never_promotes_missing_engines() {
        let readiness = drawing_readiness(&DrawingSecurityPolicy::default(), &BTreeMap::new());
        let tikz = readiness
            .adapters
            .iter()
            .find(|adapter| adapter.language == DrawingSourceLanguage::Tikz)
            .unwrap();
        assert_eq!(tikz.level, DrawingValidationLevel::ParserAvailable);
        assert!(tikz.requires_setup);
        assert!(!readiness.compilers.iter().any(|compiler| compiler.detected));
        let pstricks = readiness
            .adapters
            .iter()
            .find(|adapter| adapter.language == DrawingSourceLanguage::Pstricks)
            .unwrap();
        assert!(pstricks.blocked);
    }

    #[test]
    fn chemfig_is_experimental_and_requires_an_explicit_lock() {
        let readiness = drawing_readiness(&DrawingSecurityPolicy::default(), &BTreeMap::new());
        let chemfig = readiness
            .package_profiles
            .iter()
            .find(|profile| profile.profile == DrawingPackageProfile::ChemFig)
            .unwrap();
        assert!(chemfig.experimental);
        assert!(!chemfig.validated);
        assert!(!chemfig.allowed);
    }
}
