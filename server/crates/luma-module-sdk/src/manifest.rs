//! The wire shape a module publishes about itself.

use serde::{Deserialize, Serialize};

/// A module's reported version. Kept as a plain string for now; a real build
/// would parse and range-check it during dependency resolution.
pub type Version = String;

/// One thing a module contributes to the running server, as a (`kind`, `id`)
/// pair. `kind` is the interface ("download-client", "indexer-engine"); `id` is
/// the concrete implementation ("rqbit", "transmission", "builtin").
///
/// The host dispatches on these today through hand-written `match`es (e.g.
/// `luma_torrent::client_for`). Recording them in the registry makes the set
/// introspectable now, and is the natural home for the dispatch table itself
/// later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub kind: String,
    pub id: String,
}

impl Capability {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self { kind: kind.into(), id: id.into() }
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
/// depended module's version must satisfy (e.g. `^1.0`). Deserializes leniently
/// from a bare `"id"` string, an `"id@range"` string, or an object
/// `{ id, version }`, so old manifests keep working.
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
                Some((id, range)) => Dependency { id: id.into(), version: Some(range.into()) },
                None => Dependency { id: s, version: None },
            },
            Repr::Obj { id, version } => Dependency { id, version },
        })
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
    /// Hard dependencies on other modules (with optional version ranges).
    /// Resolution fails if any are absent or version-incompatible; init order is
    /// a topological sort over these edges.
    #[serde(default)]
    pub depends_on: Vec<Dependency>,
    /// Soft dependencies: if a listed module is present it is initialized first
    /// (and version-checked), but the module still loads when it is absent.
    #[serde(default)]
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
