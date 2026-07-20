use std::collections::{BTreeMap, BTreeSet};

/// Directed dependency graph used to expand a local edit into affected outputs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    dependents: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    pub fn link(&mut self, source: impl Into<String>, dependent: impl Into<String>) {
        self.dependents
            .entry(source.into())
            .or_default()
            .insert(dependent.into());
    }

    /// Return the edited node and every transitively affected dependent.
    pub fn invalidate(&self, source: &str) -> BTreeSet<String> {
        let mut invalidated = BTreeSet::from([source.to_string()]);
        let mut pending = vec![source.to_string()];
        while let Some(current) = pending.pop() {
            if let Some(dependents) = self.dependents.get(&current) {
                for dependent in dependents {
                    if invalidated.insert(dependent.clone()) {
                        pending.push(dependent.clone());
                    }
                }
            }
        }
        invalidated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidation_follows_transitive_dependencies() {
        let mut graph = DependencyGraph::default();
        graph.link("formula", "semantic");
        graph.link("semantic", "render");
        assert_eq!(
            graph.invalidate("formula"),
            BTreeSet::from([
                "formula".to_string(),
                "semantic".to_string(),
                "render".to_string(),
            ])
        );
    }
}
