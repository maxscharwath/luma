//! `GET /api/people?name=…` every movie + show one person is credited in
//! (cast or key crew).
//!
//! The match runs over the metadata JSON in SQLite (see [`db::titles_by_person`]),
//! exact and case-insensitive distinct from the fuzzy full-text `/search`. The
//! ranked ids are hydrated into the same DTOs `/search` returns (so clients reuse
//! their card UI), ordered best-known work first (rating, then newest).

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::api::dto::{PersonResponse, SearchHit};
use crate::api::util::query;
use crate::db;
use crate::model::{MediaItem, Metadata, Show};
use crate::state::SharedState;

#[derive(Debug, Deserialize)]
pub struct PersonParams {
    /// The person's name (exact match, case-insensitive).
    pub name: Option<String>,
    /// Optional library scope.
    pub library: Option<String>,
}

/// `GET /api/people?name=&library=` → [`PersonResponse`].
pub async fn person(
    State(state): State<SharedState>,
    Query(p): Query<PersonParams>,
) -> Result<Response, Response> {
    let name = p.name.unwrap_or_default().trim().to_string();
    let library = p.library;

    if name.is_empty() {
        return Ok(Json(PersonResponse { name, results: Vec::new() }).into_response());
    }

    let resp = query(&state.db, move |pool| {
        let (movie_ids, show_ids) = db::titles_by_person(&pool, &name)?;
        let movies = db::get_items_by_ids(&pool, &movie_ids)?;
        let shows = db::get_shows_by_ids(&pool, &show_ids)?;
        let results = collect(movies, shows, library.as_deref());
        Ok(PersonResponse { name, results })
    })
    .await?;
    Ok(Json(resp).into_response())
}

/// Merge movies + shows into one [`SearchHit`] list, optionally scoped to a single
/// library, ordered by rating (desc) then year (desc) so a person's most notable
/// work surfaces first.
fn collect(movies: Vec<MediaItem>, shows: Vec<Show>, library: Option<&str>) -> Vec<SearchHit> {
    let in_library = |lib: &str| library.is_none_or(|want| lib == want);

    let mut rows: Vec<((f32, i32), SearchHit)> = Vec::with_capacity(movies.len() + shows.len());
    for m in movies {
        if in_library(&m.library) {
            let key = sort_key(m.metadata.as_ref(), m.year);
            rows.push((key, SearchHit::Movie { item: m }));
        }
    }
    for s in shows {
        if in_library(&s.library) {
            let key = sort_key(s.metadata.as_ref(), s.year);
            rows.push((key, SearchHit::Show { show: s }));
        }
    }
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    rows.into_iter().map(|(_, hit)| hit).collect()
}

/// `(rating, year)` sort key both default to 0 when unknown so unrated/undated
/// titles sink to the bottom.
fn sort_key(meta: Option<&Metadata>, year: Option<u32>) -> (f32, i32) {
    let rating = meta.and_then(|m| m.rating).unwrap_or(0.0);
    (rating, year.map(|y| y as i32).unwrap_or(0))
}
