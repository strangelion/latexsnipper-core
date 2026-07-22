//! Registry and sole creation entry point for inference runtimes.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use latexsnipper_foundation::{Result, SnipperError};

use crate::{
    ResolvedRuntimeVariant, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions,
    RuntimeProbe, RuntimeSession,
};

#[derive(Clone, Default)]
pub struct RuntimeRegistry {
    factories: HashMap<RuntimeKind, Arc<dyn RuntimeFactory>>,
}

impl RuntimeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_factory(factory: impl RuntimeFactory + 'static) -> Self {
        let kind = factory.kind();
        Self {
            factories: HashMap::from([(kind, Arc::new(factory) as Arc<dyn RuntimeFactory>)]),
        }
    }

    pub fn register(&mut self, factory: impl RuntimeFactory + 'static) -> Result<()> {
        self.register_arc(Arc::new(factory))
    }

    pub fn register_arc(&mut self, factory: Arc<dyn RuntimeFactory>) -> Result<()> {
        let kind = factory.kind();
        if self.factories.contains_key(&kind) {
            return Err(SnipperError::Runtime(format!(
                "runtime factory '{kind}' is already registered"
            )));
        }
        self.factories.insert(kind, factory);
        Ok(())
    }

    /// Compatibility alias for the original P0 scaffold.
    pub fn try_register(&mut self, factory: impl RuntimeFactory + 'static) -> Result<()> {
        self.register(factory)
    }

    pub fn unregister(&mut self, kind: &RuntimeKind) -> Option<Arc<dyn RuntimeFactory>> {
        self.factories.remove(kind)
    }

    pub fn get(&self, kind: &RuntimeKind) -> Option<&Arc<dyn RuntimeFactory>> {
        self.factories.get(kind)
    }

    pub fn probe_all(&self) -> BTreeMap<RuntimeKind, RuntimeProbe> {
        self.factories
            .iter()
            .map(|(kind, factory)| (kind.clone(), factory.probe()))
            .collect()
    }

    pub fn probe(&self, kind: &RuntimeKind) -> Option<RuntimeProbe> {
        self.factories.get(kind).map(|factory| factory.probe())
    }

    pub fn is_available(&self, kind: &RuntimeKind) -> bool {
        self.probe(kind).is_some_and(|probe| probe.available)
    }

    pub fn is_available_str(&self, runtime_name: &str) -> bool {
        self.is_available(&RuntimeKind::from_id(runtime_name))
    }

    pub fn registered_kinds(&self) -> Vec<RuntimeKind> {
        let mut kinds: Vec<_> = self.factories.keys().cloned().collect();
        kinds.sort();
        kinds
    }

    pub fn available_runtimes(&self) -> Vec<RuntimeKind> {
        self.registered_kinds()
            .into_iter()
            .filter(|kind| self.is_available(kind))
            .collect()
    }

    /// Return the first available runtime in caller-supplied priority order.
    pub fn resolve(&self, candidates: &[RuntimeKind]) -> Option<RuntimeKind> {
        candidates
            .iter()
            .find(|kind| self.is_available(kind))
            .cloned()
    }

    pub fn create_session(
        &self,
        kind: &RuntimeKind,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        if &artifacts.runtime != kind {
            return Err(SnipperError::Runtime(format!(
                "artifact runtime '{}' does not match selected runtime '{kind}'",
                artifacts.runtime
            )));
        }
        let factory = self
            .factories
            .get(kind)
            .ok_or_else(|| SnipperError::Runtime(format!("runtime '{kind}' is not registered")))?;
        let probe = factory.probe();
        if !probe.available {
            return Err(SnipperError::Runtime(format!(
                "runtime '{kind}' is unavailable: {}",
                probe
                    .reason_unavailable
                    .unwrap_or_else(|| "no reason reported".to_owned())
            )));
        }
        factory.create_session(artifacts, options)
    }

    pub fn create_resolved_session(
        &self,
        resolved: &ResolvedRuntimeVariant,
    ) -> Result<Box<dyn RuntimeSession>> {
        self.create_session(&resolved.runtime, &resolved.artifacts, &resolved.options)
    }

    pub fn clear_sessions(&self) {
        for factory in self.factories.values() {
            factory.clear_sessions();
        }
    }

    pub fn len(&self) -> usize {
        self.factories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}
