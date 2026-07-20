//! `GET /api/home` the generated home screen: an ordered list of [`Section`]s
//! (For You, "because you watched …", themed rows, trending, recently added),
//! assembled + de-duplicated server-side so clients render them generically.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::i18n::ReqLocale;
use crate::services::sections;
use crate::state::SharedState;
use axum::routing::get;
use axum::Router;

/// `GET /api/home` + `GET /api/home/featured`.
pub fn routes() -> Router<SharedState> {
    Router::new().route("/home", get(home)).route("/home/featured", get(featured))
}

/// `GET /api/home` (Bearer) → `Section[]`. Personalized to the caller; titles are
/// localized via `Accept-Language`.
pub async fn home(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ReqLocale(locale): ReqLocale,
) -> Response {
    let gen_state = state.clone();
    let user_id = user.id.clone();
    match query(&state.db, move |pool| {
        Ok(sections::build_home(&gen_state, &pool, locale, &user_id))
    })
    .await
    {
        Ok(list) => Json(list).into_response(),
        Err(resp) => resp,
    }
}

/// `GET /api/home/featured` (Bearer) → `SectionItem | null`. Today's hero pick
/// for the caller (multi-signal score + daily rotation, see
/// `services::sections::featured`); `null` when the catalog is empty, or (logged
/// server-side, never silently) when it cannot be read.
pub async fn featured(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    ReqLocale(locale): ReqLocale,
) -> Response {
    let gen_state = state.clone();
    let user_id = user.id.clone();
    match query(&state.db, move |pool| {
        Ok(sections::featured::pick(&gen_state, &pool, &locale, &user_id))
    })
    .await
    {
        Ok(hero) => Json(hero).into_response(),
        Err(resp) => resp,
    }
}
