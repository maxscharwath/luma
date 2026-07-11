//! The concrete side of the module host seam: `HostCtx` implemented for
//! [`AppState`]. Module crates name only the trait (in `luma-module-host`); the
//! app supplies the real DB pool, capability gating (localized via the app's
//! i18n), settings, and data dir. `Arc<AppState>` (= `SharedState`, the axum
//! router state) gets `HostCtx` for free via the blanket `Arc<T>` impl.

use std::path::Path;

use axum::http::StatusCode;
use axum::response::Response;
use luma_db::Pool;
use luma_domain::{Permission, User};
use luma_module_host::{json_error, HostCtx, HostEvent};

use crate::infra::events::ServerEvent;
use crate::services::jobs::JobKey;
use crate::state::AppState;

fn forbidden(user: &User) -> Response {
    json_error(
        StatusCode::FORBIDDEN,
        &crate::i18n::t(crate::i18n::user_locale(user), "error.permissionDenied", &[]),
    )
}

impl HostCtx for AppState {
    fn db(&self) -> &Pool {
        &self.db
    }

    fn data_dir(&self) -> &Path {
        &self.config.data_dir
    }

    fn require(&self, user: &User, perm: Permission) -> Result<(), Response> {
        if user.can(perm) {
            Ok(())
        } else {
            Err(forbidden(user))
        }
    }

    fn require_any_admin(&self, user: &User) -> Result<(), Response> {
        if user.is_any_admin() {
            Ok(())
        } else {
            Err(forbidden(user))
        }
    }

    fn lerr(&self, user: &User, status: StatusCode, key: &str) -> Response {
        json_error(status, &crate::i18n::t(crate::i18n::user_locale(user), key, &[]))
    }

    fn setting_str(&self, key: &str, default: &str) -> String {
        self.settings.get_str(key, default)
    }

    fn setting_bool(&self, key: &str, default: bool) -> bool {
        self.settings.get_bool(key, default)
    }

    fn setting_i64(&self, key: &str, default: i64) -> i64 {
        self.settings.get_i64(key, default)
    }

    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>) {
        self.settings.set_patch(&self.db, patch);
    }

    fn publish(&self, event: HostEvent) {
        let ev = match event {
            HostEvent::DownloadProgress {
                id,
                request_id,
                progress,
                down_bps,
                up_bps,
                peers,
                peers_seen,
                state,
            } => ServerEvent::DownloadProgress {
                id,
                request_id,
                progress,
                down_bps,
                up_bps,
                peers,
                peers_seen,
                state,
            },
            HostEvent::DownloadCompleted { id, title } => {
                ServerEvent::DownloadCompleted { id, title }
            }
            HostEvent::VpnStatus { connected, exit_ip, paused } => {
                ServerEvent::VpnStatus { connected, exit_ip, paused }
            }
            HostEvent::RequestUpdated { id, status } => ServerEvent::RequestUpdated { id, status },
        };
        self.events.publish(ev);
    }

    fn trigger_job(&self, key: &'static str, reason: &'static str) {
        if let Some(state) = self.shared() {
            let _ = state.jobs.trigger(state.clone(), JobKey(key), reason);
        }
    }

    fn module_enabled(&self, id: &str) -> bool {
        crate::modules::module_enabled(&self.settings, id)
    }
}
