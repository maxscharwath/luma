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
use luma_module_host::{json_error, HostCtx};

use crate::state::AppState;

/// The user's account locale for server-rendered strings (admin endpoints are
/// always authenticated, so the account preference is the right source).
fn user_locale(user: &User) -> &'static str {
    user.language
        .as_deref()
        .and_then(crate::i18n::normalize)
        .unwrap_or(crate::i18n::DEFAULT_LOCALE)
}

fn forbidden(user: &User) -> Response {
    json_error(
        StatusCode::FORBIDDEN,
        &crate::i18n::t(user_locale(user), "error.permissionDenied", &[]),
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
        if user.can(Permission::UsersManage)
            || user.can(Permission::LibraryManage)
            || user.can(Permission::SettingsManage)
            || user.can(Permission::RequestsManage)
        {
            Ok(())
        } else {
            Err(forbidden(user))
        }
    }

    fn lerr(&self, user: &User, status: StatusCode, key: &str) -> Response {
        json_error(status, &crate::i18n::t(user_locale(user), key, &[]))
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

    fn set_setting_str(&self, key: &str, value: &str) {
        let mut patch = std::collections::BTreeMap::new();
        patch.insert(key.to_string(), serde_json::Value::String(value.to_string()));
        self.settings.set_patch(&self.db, patch);
    }

    fn set_settings(&self, patch: std::collections::BTreeMap<String, serde_json::Value>) {
        self.settings.set_patch(&self.db, patch);
    }
}
