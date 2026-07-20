//! Shared `#[cfg(test)]` builders for the hero tests: hand-built metadata,
//! movies and video streams plus the fixed clock the scoring assertions reason
//! against. One copy, so `score` and the orchestrator test the same shapes.

use crate::model::{Kind, MediaItem, Metadata, SectionItem, VideoStream};
use crate::state::SharedState;

/// Fixed "now" for the scoring tests (no wall clock in an assertion).
pub(super) const NOW_MS: i64 = 1_700_000_000_000;

/// Metadata carrying just the fields the hero gates and scores on.
pub(super) fn meta(rating: Option<f32>, backdrop: bool, overview: bool) -> Metadata {
    Metadata {
        provider: "tmdb",
        tmdb_id: 1,
        imdb_id: None,
        title: None,
        tagline: None,
        overview: overview.then(|| "An epic.".to_string()),
        release_date: None,
        genres: Vec::new(),
        rating,
        poster_url: None,
        backdrop_url: backdrop.then(|| "https://img/b.jpg".to_string()),
        logo_url: None,
        theme_url: None,
        cast: Vec::new(),
        crew: Vec::new(),
        keywords: Vec::new(),
        tvdb_id: None,
        tmdb_url: String::new(),
    }
}

/// An RFC3339 stamp `ms_ago` before [`NOW_MS`] (negative = a future stamp).
pub(super) fn iso(ms_ago: i64) -> String {
    let ts = time::OffsetDateTime::from_unix_timestamp((NOW_MS - ms_ago) / 1000).unwrap();
    ts.format(&time::format_description::well_known::Rfc3339).unwrap()
}

/// A movie [`SectionItem`] with everything the hero ignores left empty.
pub(super) fn movie(
    id: &str,
    m: Option<Metadata>,
    added: &str,
    video: Option<VideoStream>,
) -> SectionItem {
    SectionItem::Movie {
        item: Box::new(MediaItem {
            id: id.into(),
            title: format!("Title {id}"),
            kind: Kind::Movie,
            year: Some(2001),
            duration_ms: None,
            container: String::new(),
            video,
            audio: None,
            audio_tracks: Vec::new(),
            subtitles: Vec::new(),
            library: "lib".into(),
            show_id: None,
            show_title: None,
            season: None,
            episode: None,
            episode_end: None,
            episode_title: None,
            rel_path: None,
            added_at: added.into(),
            metadata: m,
            abs_path: None,
            files: Vec::new(),
            default_file_id: None,
            markers: Vec::new(),
            audio_analysis: None,
        }),
    }
}

/// A video stream carrying only what [`super::score`]'s cinematic bonus reads.
pub(super) fn stream(width: u32, hdr: bool) -> VideoStream {
    VideoStream { codec: "hevc".into(), width: Some(width), height: None, hdr, bit_depth: None }
}

/// Insert a user row: `watched` and `progress` both have an FK on `users`, so a
/// history marker needs one to exist. Test-controlled literal id, inlined like
/// the other section tests (kroma-engine has no direct rusqlite dependency).
pub(super) fn seed_user(state: &SharedState, id: &str) {
    state
        .db
        .get()
        .unwrap()
        .execute(
            &format!(
                "INSERT OR IGNORE INTO users (id,email,username,password_hash,avatar_url,permissions,created_at) \
                 VALUES ('{id}','{id}@x','{id}','h',NULL,'[]','t')"
            ),
            [],
        )
        .unwrap();
}
