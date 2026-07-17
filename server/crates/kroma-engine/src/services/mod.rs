//! Use-cases / orchestration: the application's domain workflows.
//!
//! These modules coordinate the infra adapters and the database to implement
//! KROMA's behaviours scanning the library, enriching it from TMDB, demo
//! seeding, live playback/quick-connect session registries, persisted settings,
//! and the activity feed.

pub mod auth;
pub mod backup;
pub mod jobs;
pub mod llm;
pub mod loginguard;
pub mod markers;
pub mod pipeline;
pub mod scan;
pub mod enrich;
pub mod search;
pub mod sections;
pub mod quickconnect;
pub mod playback;
pub mod requests;
pub mod library_missing;
pub mod settings;
pub mod subtitles;
pub mod activity;
pub mod demo;
