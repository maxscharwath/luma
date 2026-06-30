use super::*;
use crate::domain::metadata::CastMember;

fn meta(title: &str, overview: &str, genres: &[&str], cast: &[&str]) -> Metadata {
    Metadata {
        provider: "tmdb",
        tmdb_id: 1,
        imdb_id: None,
        title: Some(title.into()),
        tagline: None,
        overview: Some(overview.into()),
        release_date: None,
        genres: genres.iter().map(|s| s.to_string()).collect(),
        rating: None,
        poster_url: None,
        backdrop_url: None,
        logo_url: None,
        theme_url: None,
        cast: cast
            .iter()
            .map(|n| CastMember { name: n.to_string(), character: None, profile_url: None })
            .collect(),
        crew: Vec::new(),
        keywords: Vec::new(),
        tvdb_id: None,
        tmdb_url: String::new(),
    }
}

fn movie(id: &str, title: &str, m: Option<Metadata>) -> MediaItem {
    MediaItem {
        id: id.into(),
        title: title.into(),
        kind: Kind::Movie,
        year: None,
        duration_ms: None,
        container: String::new(),
        video: None,
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
        added_at: String::new(),
        metadata: m,
        abs_path: None,
        files: Vec::new(),
        default_file_id: None,
        markers: Vec::new(),
    }
}

fn engine() -> SearchEngine {
    let e = SearchEngine::new().unwrap();
    let movies = vec![
        movie("1", "The Avengers", Some(meta("The Avengers", "Earth's mightiest heroes", &["Action"], &["Robert Downey Jr"]))),
        movie("2", "Amélie", Some(meta("Amélie", "A shy waitress in Paris", &["Romance"], &["Audrey Tautou"]))),
    ];
    let shows = vec![Show {
        id: "s1".into(),
        title: "Breaking Bad".into(),
        year: None,
        library: "lib".into(),
        season_count: 0,
        episode_count: 0,
        video: None,
        added_at: String::new(),
        metadata: Some(meta("Breaking Bad", "A chemistry teacher turns to crime", &["Crime", "Drama"], &["Bryan Cranston"])),
        progress: None,
    }];
    e.rebuild(&movies, &shows, &[]).unwrap();
    e
}

fn top_id(e: &SearchEngine, q: &str) -> Option<String> {
    e.search(q, 5).first().map(|h| h.id.clone())
}

#[test]
fn exact_and_fuzzy_title() {
    let e = engine();
    assert_eq!(top_id(&e, "avengers").as_deref(), Some("1"));
    assert_eq!(top_id(&e, "avengrs").as_deref(), Some("1")); // typo
}

#[test]
fn accent_folding() {
    let e = engine();
    assert_eq!(top_id(&e, "amelie").as_deref(), Some("2")); // query has no accent
}

#[test]
fn cast_and_genre_and_prefix() {
    let e = engine();
    assert_eq!(top_id(&e, "cranston").as_deref(), Some("s1"));
    assert_eq!(top_id(&e, "crime").as_deref(), Some("s1"));
    assert_eq!(top_id(&e, "brea").as_deref(), Some("s1")); // prefix
}

#[test]
fn blank_query_is_empty() {
    let e = engine();
    assert!(e.search("   ", 5).is_empty());
}
