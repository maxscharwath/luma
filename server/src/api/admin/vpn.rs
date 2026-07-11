//! `/api/admin/vpn` the managed WireGuard bridge (Proton-friendly): status
//! for the downloads-page card, write-only config upload, and a live seal
//! test. Gated on `settings.manage`. The raw `vpnProxyUrl` fallback and the
//! kill-switch toggle live in the Acquisition settings view; this endpoint
//! owns only the WireGuard secret (never returned to clients).

use std::collections::BTreeMap;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::api::extract::AuthUser;
use crate::api::util::blocking;
use crate::model::{Permission, SaveVpnBody, VpnAdminView, VpnTestResult};
use luma_vpn::Vpn;
use crate::state::SharedState;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/vpn", get(status).put(save))
        .route("/vpn/test", post(test))
}

/// `GET /api/admin/vpn`
pub async fn status(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let view = VpnAdminView {
        wg_configured: Vpn::wg_configured(&state),
        bridge_running: state.vpn.running().await,
        local_port: state.settings.get_i64("vpnLocalPort", 25345).clamp(1, 65535) as u16,
        status: state.downloads.vpn_status(),
    };
    Ok(Json(view).into_response())
}

/// `PUT /api/admin/vpn` store the WireGuard config ("" removes it) and/or
/// the local bridge port, then restart the bridge + the embedded engine so
/// the new tunnel applies immediately.
pub async fn save(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SaveVpnBody>,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let mut patch: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(wg) = body.wg_config {
        patch.insert("vpnWgConfig".into(), json!(wg.trim()));
    }
    if let Some(port) = body.local_port {
        patch.insert("vpnLocalPort".into(), json!(port));
    }
    if !patch.is_empty() {
        state.settings.set_patch(&state.db, patch);
        state.vpn.apply(&state).await;
        state.downloads.start_rqbit(&state).await;
    }
    Ok(Json(json!({ "ok": true, "wgConfigured": Vpn::wg_configured(&state) })).into_response())
}

/// `POST /api/admin/vpn/test` run the seal probe now (also drives the gate).
pub async fn test(
    State(state): State<SharedState>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    super::require(&user, Permission::SettingsManage)?;
    let result = blocking(move || {
        Ok(match state.downloads.vpn_check(&state) {
            Some(check) => VpnTestResult {
                sealed: check.sealed(),
                proxied_ip: check.proxied_ip,
                direct_ip: check.direct_ip,
                error: check.error,
            },
            None => VpnTestResult {
                sealed: false,
                proxied_ip: None,
                direct_ip: None,
                error: Some("no VPN proxy configured".into()),
            },
        })
    })
    .await?;
    Ok(Json(result).into_response())
}
