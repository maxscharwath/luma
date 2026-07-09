//! WebAuthn passkeys: register credentials (authenticated, from the account
//! page) and sign in with them (public, passwordless).
//!
//! The relying-party (RP id + origin) is derived per-request from the browser's
//! `Origin` header, so the same server works across LAN host names and the
//! optional public HTTPS domain without static config each passkey is bound to
//! the origin it was created on (a WebAuthn invariant). Browsers only expose the
//! WebAuthn API in a secure context (HTTPS or localhost), so the web client
//! gates the UI on that; these handlers just serve the ceremonies.
//!
//! The short-lived ceremony state (the challenge webauthn-rs hands back at
//! "start" and needs again at "finish") is held in-process, keyed by a random
//! id returned to the client mirrors the login/PIN guards' in-memory model,
//! fine for a single-binary self-hosted server.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use webauthn_rs::prelude::{
    CredentialID, DiscoverableAuthentication, DiscoverableKey, Passkey, PasskeyRegistration,
    PublicKeyCredential, RegisterPublicKeyCredential, Url, Uuid, Webauthn, WebauthnBuilder,
};

use crate::api::accounts::{issue_tokens, user_agent};
use crate::api::error::lerr;
use crate::api::extract::AuthUser;
use crate::api::util::query;
use crate::db;
use crate::i18n::{self, ReqLocale};
use crate::services::auth;
use crate::state::SharedState;

/// Passkey routes. The `/auth/me/*` ones self-gate via [`AuthUser`]; the
/// `/auth/passkeys/authenticate/*` pair is public (it *is* the login).
pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/auth/me/passkeys", get(list))
        .route("/auth/me/passkeys/:id", axum::routing::delete(remove))
        .route("/auth/me/passkeys/register/start", post(register_start))
        .route("/auth/me/passkeys/register/finish", post(register_finish))
        .route("/auth/passkeys/authenticate/start", post(authenticate_start))
        .route("/auth/passkeys/authenticate/finish", post(authenticate_finish))
}

// ----- in-memory ceremony store -----------------------------------------------

/// How long a started ceremony may sit before it's finished (matches the
/// authenticator timeout in the challenge).
const CEREMONY_TTL_SECS: i64 = 300;

enum Ceremony {
    /// A registration in progress, bound to the account that started it.
    Register { user_id: String, reg: PasskeyRegistration },
    /// A usernameless (discoverable) authentication in progress. The account is
    /// only known once the browser returns which credential was used.
    Discover { auth: DiscoverableAuthentication },
}

struct Entry {
    expires: i64,
    ceremony: Ceremony,
}

static CEREMONIES: LazyLock<Mutex<HashMap<String, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// Store a started ceremony (pruning expired ones) → its lookup id.
fn stash(ceremony: Ceremony) -> String {
    let id = auth::random_token();
    if let Ok(mut m) = CEREMONIES.lock() {
        let n = now();
        m.retain(|_, e| e.expires > n);
        m.insert(id.clone(), Entry { expires: n + CEREMONY_TTL_SECS, ceremony });
    }
    id
}

/// Consume a ceremony by id (single-use), or `None` if unknown/expired.
fn take(id: &str) -> Option<Ceremony> {
    let mut m = CEREMONIES.lock().ok()?;
    let e = m.remove(id)?;
    (e.expires > now()).then_some(e.ceremony)
}

// ----- relying-party helpers --------------------------------------------------

/// A stable per-account WebAuthn user handle, derived from the account id (which
/// isn't itself a UUID). Deterministic so re-registration keeps the same handle.
fn user_uuid(user_id: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, user_id.as_bytes())
}

/// Build a [`Webauthn`] whose RP id/origin match the request's `Origin`. Errors
/// as an HTTP response when the header is missing/unusable or the origin isn't a
/// valid RP (e.g. plain HTTP on a non-local host, which browsers reject anyway).
fn relying_party(headers: &HeaderMap, loc: &str) -> Result<Webauthn, Response> {
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| lerr(loc, StatusCode::BAD_REQUEST, "passkey.originMissing"))?;
    let url =
        Url::parse(origin).map_err(|_| lerr(loc, StatusCode::BAD_REQUEST, "passkey.originInvalid"))?;
    let rp_id = url
        .host_str()
        .ok_or_else(|| lerr(loc, StatusCode::BAD_REQUEST, "passkey.originInvalid"))?
        .to_string();
    WebauthnBuilder::new(&rp_id, &url)
        .and_then(|b| b.allow_any_port(true).rp_name("LUMA").build())
        .map_err(|_| lerr(loc, StatusCode::BAD_REQUEST, "passkey.originInvalid"))
}

/// Deserialize the stored `Passkey` blobs into `(display id, Passkey)` pairs.
fn parse_passkeys(blobs: &[String]) -> Vec<(String, Passkey)> {
    blobs
        .iter()
        .filter_map(|j| serde_json::from_str::<Passkey>(j).ok())
        .map(|pk| (hex::encode(pk.cred_id()), pk))
        .collect()
}

// ----- registration (authenticated) -------------------------------------------

/// `POST /api/auth/me/passkeys/register/start` (Bearer) → `{ ceremonyId, options }`.
/// `options` is the WebAuthn creation challenge the browser feeds to
/// `navigator.credentials.create`.
pub async fn register_start(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    headers: HeaderMap,
    AuthUser(user): AuthUser,
) -> Response {
    let webauthn = match relying_party(&headers, loc) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    // Exclude the account's existing credentials so the same authenticator can't
    // be enrolled twice.
    let uid = user.id.clone();
    let blobs = match query(&state.db, move |pool| db::passkey_credentials(&pool, &uid)).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let exclude: Vec<CredentialID> =
        parse_passkeys(&blobs).into_iter().map(|(_, pk)| pk.cred_id().clone()).collect();
    let exclude = (!exclude.is_empty()).then_some(exclude);

    match webauthn.start_passkey_registration(
        user_uuid(&user.id),
        &user.username,
        &user.username,
        exclude,
    ) {
        Ok((ccr, reg)) => {
            let ceremony_id = stash(Ceremony::Register { user_id: user.id, reg });
            Json(json!({ "ceremonyId": ceremony_id, "options": ccr })).into_response()
        }
        Err(_) => lerr(loc, StatusCode::BAD_REQUEST, "passkey.startFailed"),
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterFinishBody {
    #[serde(rename = "ceremonyId")]
    pub ceremony_id: String,
    /// Friendly label for the credential (falls back to a default if blank).
    #[serde(default)]
    pub name: String,
    /// The `PublicKeyCredential` produced by `navigator.credentials.create`.
    pub credential: RegisterPublicKeyCredential,
}

/// `POST /api/auth/me/passkeys/register/finish` (Bearer) → `{ id, name }`.
/// Verifies the attestation against the stashed challenge and stores the
/// credential on the account.
pub async fn register_finish(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    headers: HeaderMap,
    AuthUser(user): AuthUser,
    Json(body): Json<RegisterFinishBody>,
) -> Response {
    let webauthn = match relying_party(&headers, loc) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    let Some(Ceremony::Register { user_id, reg }) = take(&body.ceremony_id) else {
        return lerr(loc, StatusCode::BAD_REQUEST, "passkey.ceremonyExpired");
    };
    // A ceremony is bound to the account that started it.
    if user_id != user.id {
        return lerr(loc, StatusCode::FORBIDDEN, "passkey.ceremonyExpired");
    }

    let passkey = match webauthn.finish_passkey_registration(&body.credential, &reg) {
        Ok(pk) => pk,
        Err(_) => return lerr(loc, StatusCode::BAD_REQUEST, "passkey.registerFailed"),
    };
    let id = hex::encode(passkey.cred_id());
    let cred_json = match serde_json::to_string(&passkey) {
        Ok(j) => j,
        Err(_) => return lerr(loc, StatusCode::INTERNAL_SERVER_ERROR, "error.internal"),
    };
    let name = {
        let n = body.name.trim();
        if n.is_empty() {
            i18n::t(loc, "passkey.defaultName", &[])
        } else {
            n.to_string()
        }
    };

    let (uid, id_db, name_db, cred_db) = (user.id.clone(), id.clone(), name.clone(), cred_json);
    let created_at =
        match query(&state.db, move |pool| db::insert_passkey(&pool, &id_db, &uid, &name_db, &cred_db)).await {
            Ok(ts) => ts,
            Err(resp) => return resp,
        };
    // Echo the full stored row so the client's list can update without a re-read.
    Json(super::dto::PasskeyInfo { id, name, created_at, last_used: None }).into_response()
}

// ----- list / remove (authenticated) ------------------------------------------

/// `GET /api/auth/me/passkeys` (Bearer) → `PasskeyInfo[]`, newest first.
pub async fn list(State(state): State<SharedState>, AuthUser(user): AuthUser) -> Response {
    let uid = user.id.clone();
    match query(&state.db, move |pool| db::list_passkeys(&pool, &uid)).await {
        Ok(rows) => {
            let out: Vec<super::dto::PasskeyInfo> = rows
                .into_iter()
                .map(|r| super::dto::PasskeyInfo {
                    id: r.id,
                    name: r.name,
                    created_at: r.created_at,
                    last_used: r.last_used,
                })
                .collect();
            Json(out).into_response()
        }
        Err(resp) => resp,
    }
}

/// `DELETE /api/auth/me/passkeys/:id` (Bearer) → 204. Removes one of the
/// account's own passkeys. `404` if the id isn't one of theirs.
pub async fn remove(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let uid = user.id.clone();
    match query(&state.db, move |pool| db::delete_passkey(&pool, &uid, &id)).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => lerr(loc, StatusCode::NOT_FOUND, "passkey.notFound"),
        Err(resp) => resp,
    }
}

// ----- authentication (public, usernameless / discoverable) -------------------

/// `POST /api/auth/passkeys/authenticate/start` → `{ ceremonyId, options }`. No
/// identifier: the challenge allows any of this server's passkeys, and the
/// browser lets the user pick which account to sign in as.
pub async fn authenticate_start(
    State(_state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    headers: HeaderMap,
) -> Response {
    let webauthn = match relying_party(&headers, loc) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    match webauthn.start_discoverable_authentication() {
        Ok((rcr, auth)) => {
            let ceremony_id = stash(Ceremony::Discover { auth });
            Json(json!({ "ceremonyId": ceremony_id, "options": rcr })).into_response()
        }
        Err(_) => lerr(loc, StatusCode::BAD_REQUEST, "passkey.startFailed"),
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthFinishBody {
    #[serde(rename = "ceremonyId")]
    pub ceremony_id: String,
    /// The assertion from `navigator.credentials.get`.
    pub credential: PublicKeyCredential,
}

/// Resolve the account behind a discoverable assertion's user handle (a v5 UUID
/// derived from the account id), matching against the ids that own passkeys.
async fn account_for_handle(state: &SharedState, handle: Uuid) -> Option<String> {
    let ids = query(&state.db, |pool| db::passkey_user_ids(&pool)).await.ok()?;
    ids.into_iter().find(|id| user_uuid(id) == handle)
}

/// `POST /api/auth/passkeys/authenticate/finish` `{ ceremonyId, credential }` →
/// `{ token, accessToken, user }` (same shape as password login). Identifies the
/// account from the assertion, verifies it, advances the credential's counter,
/// and opens a session.
pub async fn authenticate_finish(
    State(state): State<SharedState>,
    ReqLocale(loc): ReqLocale,
    headers: HeaderMap,
    Json(body): Json<AuthFinishBody>,
) -> Response {
    let webauthn = match relying_party(&headers, loc) {
        Ok(w) => w,
        Err(resp) => return resp,
    };
    let Some(Ceremony::Discover { auth }) = take(&body.ceremony_id) else {
        return lerr(loc, StatusCode::BAD_REQUEST, "passkey.ceremonyExpired");
    };

    // The browser tells us which user handle + credential were used; map the
    // handle back to a local account.
    let Ok((handle, _)) = webauthn.identify_discoverable_authentication(&body.credential) else {
        return lerr(loc, StatusCode::UNAUTHORIZED, "passkey.authFailed");
    };
    let Some(user_id) = account_for_handle(&state, handle).await else {
        return lerr(loc, StatusCode::UNAUTHORIZED, "passkey.authFailed");
    };

    // Load that account's passkeys and verify the assertion against them.
    let uid = user_id.clone();
    let blobs = match query(&state.db, move |pool| db::passkey_credentials(&pool, &uid)).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let mut passkeys = parse_passkeys(&blobs);
    let keys: Vec<DiscoverableKey> = passkeys.iter().map(|(_, pk)| DiscoverableKey::from(pk)).collect();
    let result = match webauthn.finish_discoverable_authentication(&body.credential, auth, &keys) {
        Ok(r) => r,
        Err(_) => return lerr(loc, StatusCode::UNAUTHORIZED, "passkey.authFailed"),
    };

    // Persist the matched credential's advanced counter (replay defence) + stamp
    // it used. Best-effort a DB hiccup shouldn't block an otherwise valid login.
    let matched = hex::encode(result.cred_id());
    if let Some((id, pk)) = passkeys.iter_mut().find(|(id, _)| *id == matched) {
        let changed = pk.update_credential(&result) == Some(true);
        let cred_json = if changed { serde_json::to_string(pk).ok() } else { None };
        let id_db = id.clone();
        let _ =
            query(&state.db, move |pool| db::touch_passkey(&pool, &id_db, cred_json.as_deref())).await;
    }

    // Mint the session for the resolved account.
    let user = match query(&state.db, move |pool| db::user_by_id(&pool, &user_id)).await {
        Ok(Some(u)) => u,
        Ok(None) => return lerr(loc, StatusCode::UNAUTHORIZED, "passkey.authFailed"),
        Err(resp) => return resp,
    };
    issue_tokens(state, user, user_agent(&headers)).await
}
