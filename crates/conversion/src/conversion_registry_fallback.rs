//! Explicit availability-only fallback; executed handlers are never retried.
use super::*;

const MAX_FALLBACKS: usize = 3;

/// Ordered alternatives, not permission to install, enable or retry a handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversionFallbackPolicy {
    pub schema_version: u16,
    pub backends: Vec<String>,
}

impl Default for ConversionFallbackPolicy {
    fn default() -> Self {
        Self {
            schema_version: CONVERSION_REGISTRY_VERSION,
            backends: Vec::new(),
        }
    }
}

impl ConversionFallbackPolicy {
    /// Validate without touching any handler, dependency or package store.
    pub fn validate_for_backend(&self, backend: Option<&str>) -> RegistryResult<()> {
        validate_policy(backend.unwrap_or(BUILTIN_CONVERSION_BACKEND), self)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConversionFallbackAttemptStatus {
    Unavailable { error: ConversionRegistryError },
    Rejected { error: ConversionRegistryError },
    Converted,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionFallbackAttempt {
    pub backend: String,
    pub backend_version: Option<String>,
    pub configuration_sha256: Option<String>,
    pub status: ConversionFallbackAttemptStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConversionFallbackOutcome {
    Converted {
        result: Box<RegisteredConversionResult>,
    },
    Rejected {
        error: ConversionRegistryError,
    },
}

/// A read-only decision record, never a host write permit.
/// Cache consumers must bind the decision digest AND the host context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionFallbackReport {
    pub schema_version: u16,
    pub core_version: String,
    pub requested_backend: String,
    pub route: Option<ConversionBackendRoute>,
    /// Absent when policy validation failed before any backend was considered.
    pub policy_sha256: Option<String>,
    /// Absent when request validation failed; hashes source and caller context.
    pub request_sha256: Option<String>,
    pub decision_sha256: Option<String>,
    pub attempts: Vec<ConversionFallbackAttempt>,
    pub outcome: ConversionFallbackOutcome,
}

impl SemanticConversionRegistry {
    /// Try only caller-listed alternatives after a registry-owned availability
    /// refusal. Handler/validator failures (even forged availability codes),
    /// trust revocation, cancellation, timeout and budgets are terminal.
    /// The input, formats, mode and context never change between alternatives.
    pub fn convert_with_fallback(
        &self,
        request: &RegisteredConversionRequest,
        policy: &ConversionFallbackPolicy,
    ) -> ConversionFallbackReport {
        let primary = request
            .backend
            .as_deref()
            .unwrap_or(BUILTIN_CONVERSION_BACKEND);
        let requested_backend: String = primary.chars().take(96).collect();
        let mut report = ConversionFallbackReport {
            schema_version: CONVERSION_REGISTRY_VERSION,
            core_version: env!("CARGO_PKG_VERSION").into(),
            requested_backend,
            route: None,
            policy_sha256: None,
            request_sha256: None,
            decision_sha256: None,
            attempts: Vec::new(),
            outcome: rejected(error(
                ConversionRegistryErrorCode::InvalidDeclaration,
                "invalid fallback policy",
            )),
        };
        if let Err(failure) = validate_policy(primary, policy) {
            report.outcome = rejected(failure);
            return report;
        }
        let policy_bytes =
            serde_json::to_vec(&(CONVERSION_REGISTRY_VERSION, self.target, primary, policy))
                .expect("bounded fallback policy has serializable fields");
        report.policy_sha256 = Some(digest(&policy_bytes));
        if let Err(failure) = self.validate_request(request) {
            report.outcome = rejected(failure);
            return finish(report);
        }
        let request_bytes = serde_json::to_vec(&(
            request.schema_version,
            env!("CARGO_PKG_VERSION"),
            self.target,
            &request.input,
            &request.output,
            request.mode,
            digest(request.content.as_bytes()),
            &request.context_sha256,
        ))
        .expect("validated request identity has serializable fields");
        report.request_sha256 = Some(digest(&request_bytes));

        let route = ConversionBackendRoute {
            input: request.input.clone(),
            output: request.output.clone(),
            mode: request.mode,
            target: self.target,
        };
        report.route = Some(route.clone());
        let mut selected = request.clone();
        for id in std::iter::once(primary).chain(policy.backends.iter().map(String::as_str)) {
            let mut attempt = self.fallback_attempt(id);
            if let Some(failure) = self.fallback_unavailability(id, &route) {
                report.outcome = rejected(failure.clone());
                attempt.status = ConversionFallbackAttemptStatus::Unavailable { error: failure };
                report.attempts.push(attempt);
                continue;
            }
            selected.backend = Some(id.to_string());
            // Availability was decided by the registry, not by a handler error code.
            // Revalidate before execution; callbacks cannot request another attempt.
            match self.convert(&selected) {
                Ok(result) => {
                    attempt.status = ConversionFallbackAttemptStatus::Converted;
                    report.outcome = ConversionFallbackOutcome::Converted {
                        result: Box::new(result),
                    };
                }
                Err(failure) => {
                    attempt.status = ConversionFallbackAttemptStatus::Rejected {
                        error: failure.clone(),
                    };
                    report.outcome = rejected(failure);
                }
            }
            report.attempts.push(attempt);
            return finish(report);
        }
        finish(report)
    }

    fn fallback_attempt(&self, id: &str) -> ConversionFallbackAttempt {
        let (version, configuration) = if id == BUILTIN_CONVERSION_BACKEND {
            (
                Some(env!("CARGO_PKG_VERSION").to_string()),
                Some(digest(b"builtin-default")),
            )
        } else {
            self.backends.get(id).map_or((None, None), |entry| {
                (
                    Some(entry.descriptor.version.clone()),
                    Some(entry.descriptor.configuration_sha256.clone()),
                )
            })
        };
        ConversionFallbackAttempt {
            backend: id.to_string(),
            backend_version: version,
            configuration_sha256: configuration,
            status: ConversionFallbackAttemptStatus::Converted,
        }
    }

    fn fallback_unavailability(
        &self,
        id: &str,
        route: &ConversionBackendRoute,
    ) -> Option<ConversionRegistryError> {
        if id == BUILTIN_CONVERSION_BACKEND {
            return match (builtin_input(&route.input), builtin_output(&route.output)) {
                (Some(input), Some(output)) => {
                    let capability = CapabilityRegistry::formula_conversion(
                        input,
                        output,
                        route.mode,
                        self.target,
                    );
                    (!capability.available).then(|| {
                        error(
                            ConversionRegistryErrorCode::RouteUnavailable,
                            capability
                                .unavailable_reason
                                .unwrap_or("built-in route unavailable"),
                        )
                    })
                }
                _ => Some(error(
                    ConversionRegistryErrorCode::RouteUnavailable,
                    "built-in backend does not handle extension formats",
                )),
            };
        }
        let Some(entry) = self.backends.get(id) else {
            return Some(error(
                ConversionRegistryErrorCode::UnknownBackend,
                "unknown extension backend",
            ));
        };
        if !entry.enabled {
            return Some(error(
                ConversionRegistryErrorCode::BackendDisabled,
                "extension backend is disabled",
            ));
        }
        (!entry.descriptor.routes.contains(route)).then(|| {
            error(
                ConversionRegistryErrorCode::RouteUnavailable,
                "backend does not declare this route/platform/mode",
            )
        })
    }
}

fn validate_policy(primary: &str, policy: &ConversionFallbackPolicy) -> RegistryResult<()> {
    check_version(policy.schema_version)?;
    if policy.backends.len() > MAX_FALLBACKS {
        return Err(error(
            ConversionRegistryErrorCode::LimitExceeded,
            "at most three fallback backends are allowed",
        ));
    }
    let ids: Vec<_> = std::iter::once(primary)
        .chain(policy.backends.iter().map(String::as_str))
        .collect();
    for (index, id) in ids.iter().enumerate() {
        if *id != BUILTIN_CONVERSION_BACKEND && !extension_id(id) {
            return Err(error(
                ConversionRegistryErrorCode::InvalidDeclaration,
                "fallback requires canonical backend IDs",
            ));
        }
        if ids[..index].contains(id) {
            return Err(error(
                ConversionRegistryErrorCode::DuplicateRegistration,
                "fallback backends cannot repeat",
            ));
        }
    }
    Ok(())
}

fn rejected(error: ConversionRegistryError) -> ConversionFallbackOutcome {
    ConversionFallbackOutcome::Rejected { error }
}

fn finish(mut report: ConversionFallbackReport) -> ConversionFallbackReport {
    // Include actual identity/availability outcomes, not only the configured list.
    let bytes = serde_json::to_vec(&(
        report.schema_version,
        &report.core_version,
        &report.route,
        &report.policy_sha256,
        &report.request_sha256,
        &report.attempts,
        &report.outcome,
    ))
    .expect("fallback decision has serializable fields");
    report.decision_sha256 = Some(digest(&bytes));
    report
}
