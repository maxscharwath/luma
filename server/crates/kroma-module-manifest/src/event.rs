//! The open, module-authored event envelope.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An event emitted by a module.
///
/// The server's broadcast bus carries a *closed* `ServerEvent` enum today, so a
/// module cannot introduce its own event without editing that enum. This
/// envelope is the open alternative: a module publishes `{ module, tag,
/// payload }` and any subscriber filters by `module` / `tag`. The existing
/// typed events keep flowing unchanged; module events ride alongside them on
/// the same broadcast channel (serialized to JSON like the rest).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleEvent {
    /// The id of the module that emitted this.
    pub module: String,
    /// Event name within the module's namespace, e.g. "download.progress".
    pub tag: String,
    /// Free-form JSON payload.
    #[serde(default)]
    pub payload: Value,
}

impl ModuleEvent {
    pub fn new(module: impl Into<String>, tag: impl Into<String>, payload: Value) -> Self {
        Self { module: module.into(), tag: tag.into(), payload }
    }
}
