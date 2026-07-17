//! Core data model, split by domain noun. These modules are pure data types
//! (serde) with no I/O dependencies the persistence layer lives in `kroma-db`.
//!
//! Everything is also re-exported flat at the crate root below, so downstream
//! crates can write `kroma_domain::MediaItem` regardless of which noun-module a
//! type lives in (and the server's `crate::model` barrel re-exports this).

pub mod media;
pub mod metadata;
pub mod accounts;
pub mod playback;
pub mod library;
pub mod admin;
pub mod jobs;
pub mod pipeline;
pub mod naming;
pub mod requests;
pub mod section;

// Flat re-export (mirrors the server's former `model.rs`). `naming` is
// intentionally not globbed here; reach it via its module path.
pub use accounts::*;
pub use admin::*;
pub use jobs::*;
pub use library::*;
pub use media::*;
pub use metadata::*;
pub use pipeline::*;
pub use playback::*;
pub use requests::*;
pub use section::*;
