//! Persisted, runtime-editable server settings.
//!
//! `Config` (see [`crate::config`]) provides the immutable bootstrap values
//! sourced from the environment (bind address, data dir, …). This module layers
//! a *mutable* key/value store on top, persisted in the `settings` SQLite table,
//! so the admin console can change server behaviour at runtime and have it
//! survive a restart.
//!
//! Values are stored as JSON (`serde_json::Value`) keyed by a stable string. A
//! small set of keys are **functional** the server reads them to change real
//! behaviour (LAN classification, server name, transcode limits, …). The rest
//! are persisted preferences the admin UI renders but the server does not (yet)
//! enforce; those are marked `applied: false` in the schema so the UI can be
//! honest about it.
//!
//! Split into the [`store`] (map + persistence + defaults), the typed functional
//! [`accessors`] (+ library defs), and the admin view-model [`schema`].

mod accessors;
mod llm;
mod schema;
mod store;
mod subtitle_providers;

pub use accessors::*;
pub use llm::*;
pub use schema::*;
pub use store::*;
pub use subtitle_providers::*;
