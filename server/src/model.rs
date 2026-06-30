//! Core data model. The JSON shape here is a public contract web/TV clients
//! depend on it, so field names and casing must not drift.
//!
//! The types themselves now live in per-domain modules under [`crate::domain`];
//! this module re-exports them as a single flat namespace so existing
//! `crate::model::X` call sites keep resolving unchanged.

pub use crate::domain::media::*;
pub use crate::domain::metadata::*;
pub use crate::domain::accounts::*;
pub use crate::domain::playback::*;
pub use crate::domain::library::*;
pub use crate::domain::admin::*;
pub use crate::domain::jobs::*;
pub use crate::domain::section::*;
