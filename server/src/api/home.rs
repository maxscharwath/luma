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
