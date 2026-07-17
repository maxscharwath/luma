//! The concrete side of the module host seam: `HostCtx` implemented for
//! [`AppState`]. Module crates name only the trait (in `kroma-module-host`); the
//! app supplies the real DB pool, capability gating (localized via the app's
//! i18n), settings, and data dir. `Arc<AppState>` (= `SharedState`, the axum
//! router state) gets `HostCtx` for free via the blanket `Arc<T>` impl.

use std::path::Path;

use axum::http::StatusCode;
use axum::response::Response;
use kroma_db::Pool;
use kroma_domain::{Permission, User};
use kroma_module_host::{json_error, Event, HostCtx};

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

    fn publish(&self, event: Event) {
        // Merge the module's topic under the wire `type` key into its payload
        // object, matching the bus wire shape `{ "type": <topic>, ...fields }`.
        // The core stays generic: it names no module event type.
        let serde_json::Value::Object(mut obj) = event.payload else {
            tracing::warn!(topic = %event.topic, "module event payload is not a JSON object; dropping");
            return;
        };
        obj.insert("type".to_string(), serde_json::Value::String(event.topic));
        self.events.publish_value(serde_json::Value::Object(obj));
    }

    fn trigger_job(&self, key: &'static str, reason: &'static str) {
        if let Some(state) = self.shared() {
            let _ = state.jobs.trigger(state.clone(), JobKey(key), reason);
        }
    }

    fn module_enabled(&self, id: &str) -> bool {
        crate::modules::module_enabled(&self.settings, id)
    }

    fn library_folders(&self) -> Vec<kroma_module_host::LibraryFolders> {
        crate::services::settings::library_defs(&self.settings, &self.config)
            .into_iter()
            .map(|d| kroma_module_host::LibraryFolders {
                id: d.id,
                kind: d.kind,
                name: d.name,
                folders: d.folders,
            })
            .collect()
    }

    fn tmdb_api_key(&self) -> Option<String> {
        self.config.tmdb_api_key.clone()
    }

    fn metadata_language(&self) -> String {
        crate::services::settings::metadata_language(&self.settings, &self.config)
    }

    fn get_service(
        &self,
        type_id: std::any::TypeId,
    ) -> Option<std::sync::Arc<dyn std::any::Any + Send + Sync>> {
        self.services.get(&type_id).cloned()
    }
}
