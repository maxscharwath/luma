//! Core data model, split by domain noun. These modules are pure data types
//! (serde + ts-rs derives) with no I/O dependencies — the persistence layer
//! lives in [`crate::db`]. `crate::model` re-exports everything here as a flat
//! namespace for backwards-compatible call sites.

pub mod media;
pub mod metadata;
pub mod accounts;
pub mod playback;
pub mod library;
pub mod admin;
pub mod naming;
