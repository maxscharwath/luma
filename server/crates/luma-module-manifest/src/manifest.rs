//! The wire shape a module publishes about itself.

use serde::{Deserialize, Serialize};

/// A module's reported version. Kept as a plain string for now; a real build
/// would parse and range-check it during dependency resolution.
pub type Version = String;

/// One thing a module contributes to the running server, as a (`kind`, `id`)
/// pair. `kind` is the interface ("download-client", "indexer-engine"); `id` is
/// the concrete implementation ("rqbit", "transmission", "builtin").
///
/// The host dispatches on these today through hand-written `match`es (e.g. the
/// `DownloadClientRegistry` in `luma_torrent`). Recording them in the registry
/// makes the set introspectable now, and is the natural home for the dispatch
/// table itself later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub kind: String,
    pub id: String,
    /// Display name for engine capabilities (`download-client`, `indexer-engine`),
    /// shown in the admin's data-driven add-picker. Absent when the capability has
    /// no add-flow. Ignored by dependency resolution (which matches on kind+id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The add-form schema (reuses [`ConfigField`]) the admin renders for this
    /// engine. Empty when the engine has a custom [`flow`](Self::flow) or no form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ConfigField>,
    /// A discriminator for engines whose add-flow is NOT a plain field form (e.g.
    /// `"definition"` for the native Cardigann definition picker); the host page
    /// renders that flow itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
}

impl Capability {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self { kind: kind.into(), id: id.into(), label: None, fields: Vec::new(), flow: None }
    }
}

/// One admin-configurable setting a module exposes. The admin console renders a
/// control per field; the value is interpreted by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    /// "string" | "bool" | "number" | "select".
    #[serde(rename = "type")]
    pub kind: String,
    /// Default value, as a string the admin UI parses per `kind`.
    #[serde(default)]
    pub default: Option<String>,
    /// Choices for `kind == "select"`.
    #[serde(default)]
    pub options: Vec<String>,
    /// Placeholder text for a text/URL input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Render as a password input; the value is treated write-only.
    #[serde(default)]
    pub secret: bool,
    /// The field must be non-empty before the form can submit.
    #[serde(default)]
    pub required: bool,
}

/// The frontend half of a module, when it ships a Module Federation remote. The
/// remote's entry URL is derived by the server (`/modules/<id>/remoteEntry.js`),
/// so this only names the exposed module the host `loadRemote`s. Absent for
/// backend-only modules and for compile-time-bundled frontends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeRemote {
    /// The exposed module key to load (the remote's MF `exposes` name, e.g.
    /// "./module").
    pub module: String,
}

/// A dependency on another module: its `id` and an optional semver range the
/// depended module's version must satisfy (e.g. `^1.0`). In a manifest the whole
/// collection is written as a package.json-style `{ id: range }` map (see
/// [`dep_map`]); a single entry also deserializes leniently from a bare `"id"`
/// string, an `"id@range"` string, or an object `{ id, version }`, so older
/// array-form manifests keep working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Dependency {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), version: None }
    }
}

impl<'de> Deserialize<'de> for Dependency {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", untagged)]
        enum Repr {
            Str(String),
            Obj {
                id: String,
                #[serde(default)]
                version: Option<String>,
            },
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Str(s) => match s.split_once('@') {
                Some((id, range)) => Dependency { id: id.into(), version: normalize_range(range) },
                None => Dependency { id: s, version: None },
            },
            Repr::Obj { id, version } => {
                Dependency { id, version: version.as_deref().and_then(normalize_range) }
            }
        })
    }
}

/// Normalize a declared version range: a blank or `"*"` range means "no
/// constraint" (`None`); anything else is trimmed and kept. Applied to every
/// input form so `{ id, version: "*" }`, `"id@*"` and the map value `"*"` all
/// collapse to the same in-memory shape (and round-trip stably).
fn normalize_range(range: &str) -> Option<String> {
    let trimmed = range.trim();
    (!trimmed.is_empty() && trimmed != "*").then(|| trimmed.to_string())
}

/// (De)serialize a `dependsOn` / `optionalDependsOn` collection as a
/// package.json-style map `{ "<id>": "<range>" }`, where a bare `"*"` (or empty)
/// range means "any version". The legacy array form (a list of bare ids,
/// `"id@range"` strings, or `{ id, version }` objects) is still accepted on the
/// way in, so older manifests and third-party `.tar` modules keep loading.
mod dep_map {
    use std::fmt;

    use serde::de::{MapAccess, SeqAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserializer, Serializer};

    use super::Dependency;

    pub fn serialize<S: Serializer>(deps: &[Dependency], serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(deps.len()))?;
        for dep in deps {
            map.serialize_entry(&dep.id, dep.version.as_deref().unwrap_or("*"))?;
        }
        map.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Dependency>, D::Error> {
        struct DepsVisitor;

        impl<'de> Visitor<'de> for DepsVisitor {
            type Value = Vec<Dependency>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a { id: range } map or a list of dependencies")
            }

            // An explicit `null` (some manifest generators emit it for the empty
            // case) means "no dependencies", not a type error.
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Vec::new())
            }

            // Package.json-style map: each key is a module id, each value a range.
            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut out = Vec::new();
                while let Some((id, range)) = access.next_entry::<String, String>()? {
                    out.push(Dependency { id, version: super::normalize_range(&range) });
                }
                Ok(out)
            }

            // Legacy array: each item is a bare id, an "id@range", or { id, version }.
            fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(dep) = access.next_element::<Dependency>()? {
                    out.push(dep);
                }
                Ok(out)
            }
        }

        deserializer.deserialize_any(DepsVisitor)
    }
}

/// A dependency on a CAPABILITY rather than a specific module: satisfied by any
/// module whose `provides` matches `kind` (and `id` when given).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReq {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// The public description of a module.
///
/// This is the serde shape served at `GET /api/modules` and mirrored by the
/// frontend registry, so it holds no runtime handles - only data. The `id` is
/// the join key across the backend crate and the `@luma/module-<id>` frontend
/// package. Serialized camelCase so `depends_on` reaches the frontend (and a
/// wasm plugin's JSON) as `dependsOn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleManifest {
    /// Stable identifier, shared with the module's frontend package.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    pub version: Version,
    /// One-line description.
    #[serde(default)]
    pub description: String,
    /// Hard dependencies on other modules, written as a package.json-style
    /// `{ id: range }` map. Resolution fails if any are absent or version-
    /// incompatible; init order is a topological sort over these edges.
    #[serde(default, with = "dep_map", skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<Dependency>,
    /// Soft dependencies (same shape): if a listed module is present it is
    /// initialized first (and version-checked), but the module still loads when
    /// it is absent.
    #[serde(default, with = "dep_map", skip_serializing_if = "Vec::is_empty")]
    pub optional_depends_on: Vec<Dependency>,
    /// Capability dependencies: each is satisfied by any module providing it.
    #[serde(default)]
    pub requires: Vec<CapabilityReq>,
    /// Capabilities this module registers. Filled by the registry.
    #[serde(default)]
    pub provides: Vec<Capability>,
    /// Account capabilities (permissions) needed to use this module; the host
    /// hides or gates it otherwise.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Admin-configurable settings this module exposes.
    #[serde(default)]
    pub config: Vec<ConfigField>,
    /// The module's frontend remote, when it ships one (runtime-loaded modules).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fe_remote: Option<FeRemote>,
    /// First path segments of the admin routes this module owns (e.g. `["vpn"]`
    /// for `/api/admin/vpn/*`). For out-of-process modules, the core reverse-
    /// proxies `/api/admin/<prefix>/*` to the module's sidecar. Empty for
    /// compiled-in modules (mounted directly) and port-only modules.
    #[serde(default, rename = "adminPrefixes", skip_serializing_if = "Vec::is_empty")]
    pub admin_prefixes: Vec<String>,
    /// A library module: its `.lmod` ships no native binary (its code is co-linked
    /// into the processes that need it), so the supervisor registers it but spawns
    /// no process (e.g. the release-name parser). Purely informational for the UI.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
}

impl ModuleManifest {
    /// Start a manifest with the required fields; chain the builder methods for
    /// the rest.
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<Version>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: String::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            requires: Vec::new(),
            provides: Vec::new(),
            permissions: Vec::new(),
            config: Vec::new(),
            fe_remote: None,
            admin_prefixes: Vec::new(),
            library: false,
        }
    }

    pub fn describe(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn needs(mut self, module_id: impl Into<String>) -> Self {
        self.depends_on.push(Dependency::new(module_id));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depends_on_reads_the_package_json_style_map() {
        let m: ModuleManifest = serde_json::from_str(
            r#"{ "id": "a", "name": "A", "version": "1.0.0",
                 "dependsOn": { "dev.luma.torrents": "^0.1.0", "dev.luma.lib": "*" } }"#,
        )
        .unwrap();
        assert_eq!(m.depends_on.len(), 2);
        assert_eq!(m.depends_on[0], Dependency { id: "dev.luma.torrents".into(), version: Some("^0.1.0".into()) });
        // A "*" range normalizes to "no constraint".
        assert_eq!(m.depends_on[1], Dependency::new("dev.luma.lib"));
    }

    #[test]
    fn depends_on_still_reads_the_legacy_array_forms() {
        let m: ModuleManifest = serde_json::from_str(
            r#"{ "id": "a", "name": "A", "version": "1.0.0",
                 "dependsOn": ["bare", "with@^1.2", { "id": "obj", "version": ">=2" }] }"#,
        )
        .unwrap();
        assert_eq!(m.depends_on[0], Dependency::new("bare"));
        assert_eq!(m.depends_on[1], Dependency { id: "with".into(), version: Some("^1.2".into()) });
        assert_eq!(m.depends_on[2], Dependency { id: "obj".into(), version: Some(">=2".into()) });
    }

    #[test]
    fn serializes_as_a_map_and_omits_empty_collections() {
        let mut m = ModuleManifest::new("a", "A", "1.0.0");
        m.depends_on.push(Dependency { id: "lib".into(), version: Some("^1".into()) });
        m.depends_on.push(Dependency::new("plain"));
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["dependsOn"]["lib"], "^1");
        // No declared range serializes back as the wildcard.
        assert_eq!(json["dependsOn"]["plain"], "*");
        // Empty optionalDependsOn is skipped entirely (not written as {} or []).
        assert!(json.get("optionalDependsOn").is_none());

        // And the map round-trips back to the same in-memory shape.
        let back: ModuleManifest = serde_json::from_value(json).unwrap();
        assert_eq!(back.depends_on, m.depends_on);
    }

    #[test]
    fn depends_on_null_means_empty() {
        // Some generators emit `null` for the empty case; it must load as empty,
        // not error the whole manifest.
        let m: ModuleManifest = serde_json::from_str(
            r#"{ "id": "a", "name": "A", "version": "1.0.0", "dependsOn": null }"#,
        )
        .unwrap();
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn legacy_object_wildcard_version_normalizes_to_none() {
        // `{ id, version: "*" }` collapses to the same shape as the map "*" and a
        // bare id, so a save/load round-trip is a fixpoint.
        let m: ModuleManifest = serde_json::from_str(
            r#"{ "id": "a", "name": "A", "version": "1.0.0",
                 "dependsOn": [{ "id": "lib", "version": "*" }] }"#,
        )
        .unwrap();
        assert_eq!(m.depends_on[0], Dependency::new("lib"));
    }
}
