//! Release-name intelligence for the acquisition stack: parse scene/P2P
//! release titles ("Movie.2023.1080p.BluRay.x265-GROUP") into structured facts,
//! and score candidate releases against a quality profile. Pure computation,
//! zero I/O, fully self-contained (owns its own token vocabulary; the server's
//! filename-oriented `domain/naming` is a separate concern and stays as is).
//!
//! The public surface is stable from day one; [`parse_release_name`] and
//! [`score`] gain their real implementations with the indexer milestone.

use serde::{Deserialize, Serialize};

/// Video resolution tiers we distinguish (anything below 720p is rejected by
/// the decision engine, so it needs no variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Res {
    R720,
    R1080,
    R2160,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    Hevc,
    H264,
    Av1,
    Xvid,
}

/// Where the encode came from, best first (Remux = untouched disc stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Remux,
    BluRay,
    WebDl,
    WebRip,
    Hdtv,
    Cam,
}

/// Structured facts extracted from one release title.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParsedRelease {
    pub title: String,
    pub year: Option<u32>,
    pub resolution: Option<Res>,
    pub codec: Option<Codec>,
    pub source: Option<Source>,
    /// Trailing `-GROUP` tag, when present.
    pub group: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// Last episode of a multi-episode span (`S01E01-E03`).
    pub episode_end: Option<u32>,
    /// A whole-season pack (`S01` with no episode, "Season 1", "COMPLETE").
    pub full_season: bool,
    pub proper: bool,
    pub repack: bool,
    pub hdr: bool,
    pub dolby_vision: bool,
}

/// What the caller is trying to fill; drives season/episode validation and
/// size bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Movie { year: Option<u32> },
    Episode { season: u32, episode: u32 },
    /// A whole-season grab covering `episodes` aired episodes.
    Season { season: u32, episodes: u32 },
}

/// The quality profile the decision engine scores against (from server
/// settings; KROMA is HEVC-first so `prefer_hevc` defaults on there).
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub resolution: Res,
    pub prefer_hevc: bool,
    pub min_seeders: u32,
    pub max_size_bytes_movie: u64,
    pub max_size_bytes_episode: u64,
    pub required_keywords: Vec<String>,
    pub forbidden_keywords: Vec<String>,
}

/// One line of the score explanation, persisted with the grab so "why this
/// release" stays answerable later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreLine {
    pub rule: String,
    pub delta: i32,
    pub note: String,
}

/// A release the engine accepted, with its total and explanation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scored {
    pub parsed: ParsedRelease,
    pub score: i32,
    pub breakdown: Vec<ScoreLine>,
}

/// Why a release was rejected (shown in interactive search).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reject {
    pub rule: String,
    pub note: String,
}

/// The facts about a candidate the scorer needs beyond its parsed name.
#[derive(Debug, Clone, Copy, Default)]
pub struct Candidate {
    pub size_bytes: Option<u64>,
    pub seeders: Option<u32>,
    /// The indexer's configured priority, applied as a flat tiebreak bonus.
    pub indexer_priority: i32,
}

mod content;
mod parse;
mod score;

pub use content::{classify, ContentFile, ContentKind, TorrentContent};
pub use parse::parse_release_name;
pub use score::score;

#[cfg(test)]
mod tests;

pub mod module;
pub use module::MODULE;
