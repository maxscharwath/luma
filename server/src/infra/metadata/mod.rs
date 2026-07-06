//! Optional movie/show metadata enrichment via **TMDB** (The Movie Database)
//! overview, poster, genres, rating, and the TMDB + IMDb IDs.
//!
//! Like [`crate::infra::probe`] (which shells out to `ffprobe`), this shells out to
//! `curl` instead of pulling an HTTP/TLS dependency. That keeps the crate lean
//! and `rustc 1.81`-friendly and reuses a binary the runtime image already
//! ships. `--data-urlencode` makes query building safe for titles with spaces,
//! accents, and `&`.
//!
//! A free TMDB API key in `LUMA_TMDB_API_KEY` enables it. With no key the
//! helpers are inert and the server behaves exactly as before.
//!
//! The resolved wire entities ([`crate::domain::metadata::Metadata`] /
//! `CastMember`) are pure data types and live in the domain layer; this module
//! is the I/O adapter that produces them: the [`client`] (curl/JSON) and a
//! process-wide [`cache`].

mod cache;
mod client;
pub mod discover;

pub use cache::Cache;
pub use client::{curl_available, lookup, season_episodes, EpisodeArt, SeasonData, Target};
