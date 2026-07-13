//! The module registry: gathering, dependency resolution, capability lookup.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::manifest::{Capability, CapabilityReq, Dependency, ModuleManifest};
use crate::Module;

/// Handed to [`Module::register`](crate::Module::register) so a module can
/// record the capabilities it provides. The registry attributes everything
/// recorded here to the module currently being registered.
#[derive(Default)]
pub struct ModuleRegistration {
    capabilities: Vec<Capability>,
}

impl ModuleRegistration {
    /// Declare that this module provides the `(kind, id)` capability, e.g.
    /// `reg.provide("download-client", "rqbit")`.
    pub fn provide(&mut self, kind: impl Into<String>, id: impl Into<String>) -> &mut Self {
        self.capabilities.push(Capability::new(kind, id));
        self
    }

    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }
}

struct Entry {
    manifest: ModuleManifest,
    module: Box<dyn Module>,
}

/// Why a module graph could not be brought up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// A hard dependency names an id no registered module provides.
    MissingDependency { module: String, needs: String },
    /// A dependency's version range is not satisfied by the registered module.
    VersionMismatch { module: String, needs: String, req: String, found: String },
    /// A capability dependency is satisfied by no registered module.
    UnsatisfiedCapability { module: String, kind: String, id: Option<String> },
    /// Two modules registered the same id.
    DuplicateId(String),
    /// The dependency graph has a cycle; these ids are involved.
    Cycle(Vec<String>),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::MissingDependency { module, needs } => {
                write!(f, "module {module:?} depends on {needs:?}, which is not registered")
            }
            ResolveError::VersionMismatch { module, needs, req, found } => write!(
                f,
                "module {module:?} needs {needs:?} {req} but {found} is registered",
            ),
            ResolveError::UnsatisfiedCapability { module, kind, id } => match id {
                Some(id) => write!(f, "module {module:?} needs capability {kind:?}:{id:?}, which no module provides"),
                None => write!(f, "module {module:?} needs capability {kind:?}, which no module provides"),
            },
            ResolveError::DuplicateId(id) => write!(f, "two modules registered the id {id:?}"),
            ResolveError::Cycle(ids) => write!(f, "module dependency cycle among {ids:?}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// The set of modules the host knows about.
#[derive(Default)]
pub struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a module. Its [`register`](crate::Module::register) hook runs
    /// immediately so the capabilities land on its manifest.
    pub fn register(&mut self, module: Box<dyn Module>) -> &mut Self {
        let mut reg = ModuleRegistration::default();
        module.register(&mut reg);
        let mut manifest = module.manifest();
        // Only let register() override the manifest's declared capabilities when
        // it actually contributed some; a module that declares `provides` in its
        // module.json and keeps the default no-op register() keeps them.
        if !reg.capabilities.is_empty() {
            manifest.provides = reg.capabilities;
        }
        self.entries.push(Entry { manifest, module });
        self
    }

    /// Every module manifest, in registration order.
    pub fn manifests(&self) -> Vec<ModuleManifest> {
        self.entries.iter().map(|e| e.manifest.clone()).collect()
    }

    /// The manifest of the module that provides a given capability, if any.
    pub fn provider_of(&self, kind: &str, id: &str) -> Option<&ModuleManifest> {
        self.entries
            .iter()
            .map(|e| &e.manifest)
            .find(|m| m.provides.iter().any(|c| c.kind == kind && c.id == id))
    }

    /// The packaged icon of the module with this id, if it ships one.
    pub fn icon_of(&self, id: &str) -> Option<crate::ModuleIcon> {
        self.entries.iter().find(|e| e.manifest.id == id).and_then(|e| e.module.icon())
    }

    /// Validate the graph and return module ids in initialization order
    /// (dependencies first). Fails on a duplicate id, a missing / version-
    /// incompatible dependency, an unsatisfied capability, or a cycle.
    pub fn resolve(&self) -> Result<Vec<String>, ResolveError> {
        let mut seen = HashSet::new();
        for e in &self.entries {
            if !seen.insert(e.manifest.id.as_str()) {
                return Err(ResolveError::DuplicateId(e.manifest.id.clone()));
            }
        }
        let edges = self.dependency_edges()?;
        self.topo_sort(&edges)
    }

    /// For each module, the ids it must be initialized AFTER, validating along
    /// the way: every hard dependency must exist and satisfy its version range;
    /// a present optional dependency also adds an ordering edge (+ version check);
    /// every capability dependency must have a provider (edge to it). Absent
    /// optional deps are simply skipped.
    fn dependency_edges(&self) -> Result<HashMap<String, Vec<String>>, ResolveError> {
        let index: HashMap<&str, &ModuleManifest> =
            self.entries.iter().map(|e| (e.manifest.id.as_str(), &e.manifest)).collect();
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        for e in &self.entries {
            let m = &e.manifest;
            let mut deps: Vec<String> = Vec::new();
            for dep in &m.depends_on {
                match index.get(dep.id.as_str()) {
                    None => {
                        return Err(ResolveError::MissingDependency {
                            module: m.id.clone(),
                            needs: dep.id.clone(),
                        })
                    }
                    Some(target) => {
                        check_version(m, dep, target)?;
                        deps.push(dep.id.clone());
                    }
                }
            }
            for dep in &m.optional_depends_on {
                if let Some(target) = index.get(dep.id.as_str()) {
                    check_version(m, dep, target)?;
                    deps.push(dep.id.clone());
                }
            }
            for req in &m.requires {
                match self.provider_for(req) {
                    None => {
                        return Err(ResolveError::UnsatisfiedCapability {
                            module: m.id.clone(),
                            kind: req.kind.clone(),
                            id: req.id.clone(),
                        })
                    }
                    Some(provider) if provider != m.id => deps.push(provider),
                    _ => {} // self-provided: no edge
                }
            }
            deps.sort();
            deps.dedup();
            edges.insert(m.id.clone(), deps);
        }
        Ok(edges)
    }

    /// The id of a registered module satisfying a capability requirement, if any.
    fn provider_for(&self, req: &CapabilityReq) -> Option<String> {
        self.entries
            .iter()
            .find(|e| {
                e.manifest
                    .provides
                    .iter()
                    .any(|c| c.kind == req.kind && req.id.as_deref().is_none_or(|id| c.id == id))
            })
            .map(|e| e.manifest.id.clone())
    }

    /// Kahn's algorithm over the resolved dependency edges. Ready nodes are
    /// drained in registration order so the output is deterministic.
    fn topo_sort(&self, edges: &HashMap<String, Vec<String>>) -> Result<Vec<String>, ResolveError> {
        let ids: Vec<&str> = self.entries.iter().map(|e| e.manifest.id.as_str()).collect();
        let mut indegree: HashMap<&str, usize> = ids.iter().map(|&id| (id, 0usize)).collect();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in &self.entries {
            let m = e.manifest.id.as_str();
            for dep in edges.get(m).into_iter().flatten() {
                *indegree.get_mut(m).unwrap() += 1;
                dependents.entry(dep.as_str()).or_default().push(m);
            }
        }

        let mut queue: Vec<&str> = ids.iter().copied().filter(|id| indegree[id] == 0).collect();
        let mut order: Vec<String> = Vec::with_capacity(self.entries.len());
        let mut cursor = 0;
        while cursor < queue.len() {
            let m = queue[cursor];
            cursor += 1;
            order.push(m.to_string());
            if let Some(deps) = dependents.get(m) {
                for &d in deps {
                    let n = indegree.get_mut(d).unwrap();
                    *n -= 1;
                    if *n == 0 {
                        queue.push(d);
                    }
                }
            }
        }

        if order.len() != self.entries.len() {
            let stuck: Vec<String> = ids
                .iter()
                .filter(|id| !order.iter().any(|o| o == *id))
                .map(|s| s.to_string())
                .collect();
            return Err(ResolveError::Cycle(stuck));
        }
        Ok(order)
    }
}

/// Enforce a dependency's version range against the target module, if declared.
/// Ranges use dtolnay `semver` syntax (caret / tilde / comparators, `,`-separated
/// for AND), NOT npm wildcard forms like `1.x` or space-separated ANDs. Permissive:
/// an unparseable range or target version is not treated as a mismatch, so a
/// typo'd range is ignored rather than taking the whole module graph down.
fn check_version(
    module: &ModuleManifest,
    dep: &Dependency,
    target: &ModuleManifest,
) -> Result<(), ResolveError> {
    let Some(range) = &dep.version else {
        return Ok(());
    };
    if let (Ok(req), Ok(found)) =
        (semver::VersionReq::parse(range), semver::Version::parse(&target.version))
    {
        if !req.matches(&found) {
            return Err(ResolveError::VersionMismatch {
                module: module.id.clone(),
                needs: dep.id.clone(),
                req: range.clone(),
                found: target.version.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleManifest;

    /// A stand-in module for graph tests.
    struct Fake {
        manifest: ModuleManifest,
        provides: Vec<(&'static str, &'static str)>,
    }

    impl Fake {
        fn boxed(
            id: &str,
            deps: &[&str],
            provides: &[(&'static str, &'static str)],
        ) -> Box<dyn Module> {
            let mut manifest = ModuleManifest::new(id, id, "0.1.0");
            for d in deps {
                manifest = manifest.needs(*d);
            }
            Box::new(Fake { manifest, provides: provides.to_vec() })
        }
    }

    impl Module for Fake {
        fn manifest(&self) -> ModuleManifest {
            self.manifest.clone()
        }
        fn register(&self, reg: &mut ModuleRegistration) {
            for (kind, id) in &self.provides {
                reg.provide(*kind, *id);
            }
        }
    }

    fn index_of(order: &[String], id: &str) -> usize {
        order.iter().position(|o| o == id).expect("id present in order")
    }

    #[test]
    fn resolves_dependencies_before_dependents() {
        let mut reg = Registry::new();
        reg.register(Fake::boxed("a", &["b"], &[]));
        reg.register(Fake::boxed("b", &["c"], &[]));
        reg.register(Fake::boxed("c", &[], &[]));

        let order = reg.resolve().expect("graph resolves");
        assert!(index_of(&order, "c") < index_of(&order, "b"));
        assert!(index_of(&order, "b") < index_of(&order, "a"));
    }

    #[test]
    fn missing_dependency_is_reported() {
        let mut reg = Registry::new();
        reg.register(Fake::boxed("torrents", &["nope"], &[]));
        assert_eq!(
            reg.resolve(),
            Err(ResolveError::MissingDependency {
                module: "torrents".into(),
                needs: "nope".into(),
            })
        );
    }

    #[test]
    fn duplicate_id_is_reported() {
        let mut reg = Registry::new();
        reg.register(Fake::boxed("dup", &[], &[]));
        reg.register(Fake::boxed("dup", &[], &[]));
        assert_eq!(reg.resolve(), Err(ResolveError::DuplicateId("dup".into())));
    }

    #[test]
    fn cycle_is_reported() {
        let mut reg = Registry::new();
        reg.register(Fake::boxed("a", &["b"], &[]));
        reg.register(Fake::boxed("b", &["a"], &[]));
        match reg.resolve() {
            Err(ResolveError::Cycle(ids)) => {
                assert!(ids.contains(&"a".to_string()) && ids.contains(&"b".to_string()));
            }
            other => panic!("expected a cycle, got {other:?}"),
        }
    }

    #[test]
    fn register_populates_provides_and_lookup() {
        let mut reg = Registry::new();
        reg.register(Fake::boxed(
            "torrents",
            &[],
            &[("download-client", "rqbit"), ("download-client", "transmission")],
        ));

        let manifests = reg.manifests();
        assert_eq!(manifests[0].provides.len(), 2);
        assert_eq!(reg.provider_of("download-client", "rqbit").unwrap().id, "torrents");
        assert!(reg.provider_of("download-client", "unknown").is_none());
    }

    /// A Fake whose manifest is built directly (for version / optional / capability
    /// deps the `boxed` helper can't express).
    fn boxed_manifest(manifest: ModuleManifest) -> Box<dyn Module> {
        Box::new(Fake { manifest, provides: Vec::new() })
    }

    #[test]
    fn version_range_is_enforced() {
        let mut ok = Registry::new();
        ok.register(Fake::boxed("lib", &[], &[])); // version 0.1.0
        let mut app = ModuleManifest::new("app", "app", "1.0.0");
        app.depends_on.push(Dependency { id: "lib".into(), version: Some(">=0.1".into()) });
        ok.register(boxed_manifest(app));
        assert!(ok.resolve().is_ok());

        let mut bad = Registry::new();
        bad.register(Fake::boxed("lib", &[], &[])); // version 0.1.0
        let mut app = ModuleManifest::new("app", "app", "1.0.0");
        app.depends_on.push(Dependency { id: "lib".into(), version: Some("^2".into()) });
        bad.register(boxed_manifest(app));
        assert!(matches!(bad.resolve(), Err(ResolveError::VersionMismatch { .. })));
    }

    #[test]
    fn optional_dep_is_skipped_when_absent_and_ordered_when_present() {
        // Absent optional dep: still resolves.
        let mut reg = Registry::new();
        let mut app = ModuleManifest::new("app", "app", "1.0.0");
        app.optional_depends_on.push(Dependency::new("maybe"));
        reg.register(boxed_manifest(app));
        assert!(reg.resolve().is_ok());

        // Present optional dep: ordered before the dependent.
        let mut reg = Registry::new();
        let mut app = ModuleManifest::new("app", "app", "1.0.0");
        app.optional_depends_on.push(Dependency::new("maybe"));
        reg.register(boxed_manifest(app));
        reg.register(Fake::boxed("maybe", &[], &[]));
        let order = reg.resolve().expect("resolves");
        assert!(index_of(&order, "maybe") < index_of(&order, "app"));
    }

    #[test]
    fn capability_dependency_resolves_to_a_provider() {
        // Provider present: resolves, provider ordered first.
        let mut reg = Registry::new();
        reg.register(Fake::boxed("engine", &[], &[("download-client", "rqbit")]));
        let mut app = ModuleManifest::new("app", "app", "1.0.0");
        app.requires.push(CapabilityReq { kind: "download-client".into(), id: None });
        reg.register(boxed_manifest(app));
        let order = reg.resolve().expect("resolves");
        assert!(index_of(&order, "engine") < index_of(&order, "app"));

        // No provider: unsatisfied.
        let mut reg = Registry::new();
        let mut app = ModuleManifest::new("app", "app", "1.0.0");
        app.requires.push(CapabilityReq { kind: "download-client".into(), id: None });
        reg.register(boxed_manifest(app));
        assert!(matches!(reg.resolve(), Err(ResolveError::UnsatisfiedCapability { .. })));
    }
}
