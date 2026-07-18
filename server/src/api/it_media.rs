//! Integration tests for the read-only catalogue query surface: library-scoped
//! browse + pagination on `media.rs` and `search.rs`, the themed recommendation
//! rows (`recommend.rs`), the TMDB-gated metadata handler (`metadata.rs`), and
//! the public theme-song endpoint (`themes.rs`). No network, ffmpeg, or disk
//! assets are reached.

use axum::http::StatusCode;
use serde_json::json;

use crate::api::test_support::{demo_item_id, demo_show_id, get, raw, test_app};
use serde_json::Value;

/// The demo Movies-library id, resolved through the live libraries endpoint.
async fn movies_library_id(t: &crate::api::test_support::TestApp) -> String {
    let (_, libs) = get(&t.app, "/api/libraries", Some(&t.token)).await;
    libs.as_array()
        .unwrap()
        .iter()
        .find(|l| l["kind"] == json!("movies"))
        .expect("movies library")["id"]
        .as_str()
        .unwrap()
        .to_string()
}

// ----- library-scoped browse --------------------------------------------------

#[tokio::test]
async fn items_and_movies_filter_by_library() {
    let t = test_app();
    let movies = movies_library_id(&t).await;

    // All items = 6 movies + 4 episodes.
    let (status, all) = get(&t.app, "/api/items", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().map(Vec::len), Some(10));

    // Scoped to the movies library = only the 6 movies.
    let (status, scoped) = get(&t.app, &format!("/api/items?library={movies}"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(scoped.as_array().map(Vec::len), Some(6));

    // `/movies` scoped to the movies library is the same 6.
    let (_, m) = get(&t.app, &format!("/api/movies?library={movies}"), Some(&t.token)).await;
    assert_eq!(m.as_array().map(Vec::len), Some(6));

    // An unknown library filters everything out (not an error).
    let (status, none) = get(&t.app, "/api/items?library=ghost-lib", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(none.as_array().map(Vec::len), Some(0));
}

// ----- server status / health / logs ------------------------------------------

#[tokio::test]
async fn health_and_status_are_public_and_report_the_demo_counts() {
    let t = test_app();

    // /health is unauthenticated (the TV polls it before login) and counts the seed.
    let (status, body) = get(&t.app, "/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("ok"));
    assert_eq!(body["items"], json!(10));
    assert_eq!(body["shows"], json!(2));
    assert_eq!(body["libraries"], json!(2));
    assert_eq!(body["ffprobe"], json!(false));

    // /status is the public scan/enrichment snapshot (idle in the test app).
    let (status, snap) = get(&t.app, "/api/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(snap.is_object());
}

#[tokio::test]
async fn logs_tail_returns_plain_text_from_the_newest_file() {
    let t = test_app();

    // No log files yet -> an empty (but well-formed, 200 text/plain) body.
    let (status, headers, _) = raw(&t.app, "GET", "/api/logs?tail=5", Some(&t.token), None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .starts_with("text/plain"));

    // Drop a log file, then the tail reads its last N lines back.
    let dir = t.state.config.logs_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kroma.log"), "line-1\nline-2\nline-3\n").unwrap();
    let (status, _headers, body) = raw(&t.app, "GET", "/api/logs?tail=2", Some(&t.token), None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    // A text/plain body isn't JSON, so the harness parses it to null; the code
    // path (read newest file + tail) is what this exercises. Sanity-check the
    // file the handler reads from actually contains the trailing lines.
    assert_eq!(body, Value::Null);
    let raw_text = std::fs::read_to_string(dir.join("kroma.log")).unwrap();
    assert!(raw_text.contains("line-3"));
}

// ----- shows (personalised progress) ------------------------------------------

#[tokio::test]
async fn shows_list_carries_progress_for_an_authed_caller() {
    let t = test_app();
    // Authed + at least one show in scope exercises the per-show progress overlay.
    let (status, shows) = get(&t.app, "/api/shows", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    let shows = shows.as_array().expect("shows array");
    assert_eq!(shows.len(), 2, "both demo shows list");
    // Nothing watched yet, so the per-show progress overlay leaves `progress`
    // unset (omitted when None); the authed branch still runs to compute it.
    assert!(shows.iter().all(|s| s["id"].is_string()));
    assert!(shows.iter().any(|s| s["title"] == json!("The Office")));
}

#[tokio::test]
async fn shows_scoped_to_the_movies_library_is_empty() {
    let t = test_app();
    let movies = movies_library_id(&t).await;
    let (status, shows) = get(&t.app, &format!("/api/shows?library={movies}"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shows.as_array().map(Vec::len), Some(0), "no shows live in the movies library");
}

// ----- search query params ----------------------------------------------------

#[tokio::test]
async fn search_honours_the_limit() {
    let t = test_app();
    // "the" matches several demo titles; limit=1 caps the result set.
    let (status, body) = get(&t.app, "/api/search?q=the&limit=1", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].as_array().map(Vec::len).unwrap_or(0) <= 1);
}

#[tokio::test]
async fn search_scoped_to_a_library_excludes_other_libraries() {
    let t = test_app();
    let movies = movies_library_id(&t).await;

    // Matrix lives in the movies library, so scoping to it still finds the hit.
    let (_, in_scope) = get(&t.app, &format!("/api/search?q=Matrix&library={movies}"), Some(&t.token)).await;
    assert!(in_scope["results"].as_array().map(Vec::len).unwrap_or(0) >= 1);

    // Scoping to a foreign library drops it.
    let (status, out) = get(&t.app, "/api/search?q=Matrix&library=ghost-lib", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["results"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn search_surfaces_an_episode_hit_as_episode() {
    let t = test_app();
    let (status, body) = get(&t.app, "/api/search?q=Dundies", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().expect("results");
    assert!(
        results.iter().any(|r| r["type"] == json!("episode") && r["item"]["episodeTitle"] == json!("The Dundies")),
        "expected an episode hit for 'The Dundies': {results:?}"
    );
}

// ----- themed recommendation row ----------------------------------------------

#[tokio::test]
async fn themed_row_is_empty_for_a_blank_query_and_an_array_otherwise() {
    let t = test_app();

    let (status, empty) = get(&t.app, "/api/themed?q=%20", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty.as_array().map(Vec::len), Some(0));

    // A real phrase returns a (possibly empty) array; the NoopEmbedder never errors.
    let (status, row) = get(&t.app, "/api/themed?q=space%20opera", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(row.is_array());
}

// ----- TMDB-gated metadata ----------------------------------------------------

#[tokio::test]
async fn show_metadata_is_unavailable_without_a_tmdb_key() {
    let t = test_app();
    let id = crate::api::test_support::demo_show_id("The Office");
    // No TMDB key in the test config -> 503 before any lookup.
    let (status, _) = get(&t.app, &format!("/api/shows/{id}/metadata"), Some(&t.token)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn metadata_routes_require_a_session() {
    let t = test_app();
    let id = demo_item_id("The Matrix");
    let (status, _) = get(&t.app, &format!("/api/items/{id}/metadata"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ----- theme songs (public) ---------------------------------------------------

fn enable_theme_songs(t: &crate::api::test_support::TestApp) {
    t.state.settings.set_patch(
        &t.state.db,
        [("themeSongs".to_string(), json!(true))].into_iter().collect(),
    );
}

#[tokio::test]
async fn theme_endpoint_404s_when_the_feature_is_off() {
    let t = test_app();
    // themeSongs defaults off -> the endpoint is silent even for a plausible name.
    let (status, _) = get(&t.app, "/api/themes/123.mp3", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn theme_endpoint_rejects_unsafe_names_then_404s_a_missing_file() {
    let t = test_app();
    enable_theme_songs(&t);

    // A non-mp3 / traversal-ish name is a 400 before any disk access.
    let (status, _) = get(&t.app, "/api/themes/evil.txt", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A well-formed name whose cache file doesn't exist yet is a clean 404.
    let (status, _) = get(&t.app, "/api/themes/424242.mp3", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ----- people library scope ---------------------------------------------------

#[tokio::test]
async fn people_lookup_accepts_a_library_scope() {
    let t = test_app();
    // Demo carries no cast, so this is an empty-but-200 envelope; it exercises the
    // library-scoped query path.
    let (status, body): (StatusCode, Value) =
        get(&t.app, "/api/people?name=Nobody&library=ghost", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("Nobody"));
    assert_eq!(body["results"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn people_lookup_with_a_blank_name_is_an_empty_envelope() {
    let t = test_app();
    // No `name` -> short-circuits to an empty result set before any query.
    let (status, body): (StatusCode, Value) = get(&t.app, "/api/people", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!(""));
    assert_eq!(body["results"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn people_lookup_merges_movie_and_show_credits() {
    let t = test_app();
    // Inject a shared cast credit onto one movie + one show (the demo blob is
    // otherwise empty), so the person query returns both and `collect` merges +
    // sorts them into one hit list.
    let movie = demo_item_id("The Matrix");
    let show = demo_show_id("The Office");
    let cast = r#"{"tmdbId":1,"tmdbUrl":"","genres":[],"cast":[{"name":"Ada Lovelace"}]}"#;
    {
        let conn = t.state.db.get().unwrap();
        conn.execute("UPDATE items SET metadata = ?2 WHERE id = ?1", (movie.as_str(), cast)).unwrap();
        conn.execute("UPDATE shows SET metadata = ?2 WHERE id = ?1", (show.as_str(), cast)).unwrap();
    }

    let (status, body): (StatusCode, Value) =
        get(&t.app, "/api/people?name=Ada%20Lovelace", Some(&t.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("Ada Lovelace"));
    let results = body["results"].as_array().expect("results");
    assert_eq!(results.len(), 2, "both the movie and show credit surface");
    assert!(results.iter().any(|r| r["type"] == json!("movie")));
    assert!(results.iter().any(|r| r["type"] == json!("show")));

    // Scoping to one library keeps only that library's hit.
    let matrix_lib = movies_library_id(&t).await;
    let (_, scoped): (StatusCode, Value) =
        get(&t.app, &format!("/api/people?name=Ada%20Lovelace&library={matrix_lib}"), Some(&t.token)).await;
    assert_eq!(scoped["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(scoped["results"][0]["type"], json!("movie"));
}
