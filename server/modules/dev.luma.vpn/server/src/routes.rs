//! The managed WireGuard bridge admin API (`/api/admin/vpn`), generic over any
//! [`HostCtx`] state. Moved out of the binary so the VPN module owns its whole
//! vertical: status for the downloads-page card, write-only config upload, and a
//! live seal test. Gated on `settings.manage`. The endpoint owns only the
//! WireGuard secret (never returned to clients); the raw `vpnProxyUrl` fallback
//! and the kill-switch toggle live in the Acquisition settings view.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use luma_domain::Permission;

use crate::{SaveVpnBody, VpnAdminView, VpnTestResult};
use luma_torrent::DownloadManager;
use luma_module_host::{blocking, service, AuthUser, HostCtx};

use crate::wg_configured;

use crate::Vpn;

pub fn routes<S>() -> Router<S>
where
    S: HostCtx + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/vpn", get(status::<S>).put(save::<S>))
        .route("/vpn/test", post(test::<S>))
}

/// `GET /api/admin/vpn`
async fn status<S: HostCtx + Clone + Send + Sync + 'static>(
    State(state): State<S>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    state.require(&user, Permission::SettingsManage)?;
    let bridge_running = match service::<Vpn>(&state) {
        Some(vpn) => vpn.running().await,
        None => false,
    };
    let view = VpnAdminView {
        wg_configured: wg_configured(&state),
        bridge_running,
        local_port: state.setting_i64("vpnLocalPort", 25345).clamp(1, 65535) as u16,
        status: service::<DownloadManager>(&state).and_then(|d| d.vpn_status()),
    };
    Ok(Json(view).into_response())
}

/// `PUT /api/admin/vpn` store the WireGuard config ("" removes it) and/or the
/// local bridge port, then restart the bridge + the embedded engine so the new
/// tunnel applies immediately.
async fn save<S: HostCtx + Clone + Send + Sync + 'static>(
    State(state): State<S>,
    AuthUser(user): AuthUser,
    Json(body): Json<SaveVpnBody>,
) -> Result<Response, Response> {
    state.require(&user, Permission::SettingsManage)?;
    let mut patch: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(wg) = body.wg_config {
        patch.insert("vpnWgConfig".into(), json!(wg.trim()));
    }
    if let Some(port) = body.local_port {
        patch.insert("vpnLocalPort".into(), json!(port));
    }
    if !patch.is_empty() {
        state.set_settings(patch);
        if let Some(vpn) = service::<Vpn>(&state) {
            vpn.apply(&state).await;
        }
        if let Some(downloads) = service::<DownloadManager>(&state) {
            downloads.start_rqbit(&state).await;
        }
    }
    Ok(Json(json!({ "ok": true, "wgConfigured": wg_configured(&state) })).into_response())
}

/// `POST /api/admin/vpn/test` run the seal probe now (also drives the gate).
async fn test<S: HostCtx + Clone + Send + Sync + 'static>(
    State(state): State<S>,
    AuthUser(user): AuthUser,
) -> Result<Response, Response> {
    state.require(&user, Permission::SettingsManage)?;
    let result = blocking(move || {
        Ok(match service::<DownloadManager>(&state).and_then(|d| d.vpn_check(&state)) {
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
