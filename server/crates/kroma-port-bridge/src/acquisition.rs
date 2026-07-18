//! Consumer-side client for the acquisition module's `AcquisitionSearchPort`,
//! consumed by the CORE (the `/api/requests/:id/search` + `/grab` endpoints).
//!
//! Unlike the other bridges, the PROVIDER routes live in the acquisition crate
//! (`serve.rs`): the `grab` handler needs the owned host to background the slow
//! engine add, which the generic bridge can't express. This half is just the
//! client the core resolves as `Arc<dyn AcquisitionSearchPort>`.

use kroma_module_host::HostCtx;
use kroma_module_sdk::ports::AcquisitionSearchPort;
use serde_json::json;

use crate::{call, Resolver};

pub struct AcquisitionSearchClient {
    resolve: Resolver,
}

impl AcquisitionSearchClient {
    pub fn new(resolve: Resolver) -> Self {
        Self { resolve }
    }
}

impl AcquisitionSearchPort for AcquisitionSearchClient {
    fn interactive_search(
        &self,
        _host: &dyn HostCtx,
        request_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        call(&self.resolve, "acqsearch/search", &json!({ "request_id": request_id }))
    }

    fn grab(
        &self,
        _host: &dyn HostCtx,
        request_id: &str,
        guid: &str,
        indexer_id: &str,
    ) -> anyhow::Result<String> {
        call(
            &self.resolve,
            "acqsearch/grab",
            &json!({ "request_id": request_id, "guid": guid, "indexer_id": indexer_id }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[derive(Clone)]
    struct MockHost;
    impl HostCtx for MockHost {
        fn db(&self) -> &kroma_module_sdk::db::Pool {
            unimplemented!()
        }
        fn data_dir(&self) -> &std::path::Path {
            std::path::Path::new("/tmp")
        }
        fn require(
            &self,
            _user: &kroma_module_sdk::domain::User,
            _perm: kroma_module_sdk::domain::Permission,
        ) -> Result<(), axum::response::Response> {
            Ok(())
        }
        fn require_any_admin(
            &self,
            _user: &kroma_module_sdk::domain::User,
        ) -> Result<(), axum::response::Response> {
            Ok(())
        }
        fn lerr(
            &self,
            _user: &kroma_module_sdk::domain::User,
            _status: axum::http::StatusCode,
            _key: &str,
        ) -> axum::response::Response {
            unimplemented!()
        }
        fn setting_str(&self, _key: &str, default: &str) -> String {
            default.to_string()
        }
        fn setting_bool(&self, _key: &str, default: bool) -> bool {
            default
        }
        fn setting_i64(&self, _key: &str, default: i64) -> i64 {
            default
        }
        fn set_settings(&self, _patch: std::collections::BTreeMap<String, serde_json::Value>) {}
        fn publish(&self, _event: kroma_module_host::Event) {}
        fn trigger_job(&self, _key: &'static str, _reason: &'static str) {}
        fn module_enabled(&self, _id: &str) -> bool {
            true
        }
        fn library_folders(&self) -> Vec<kroma_module_host::LibraryFolders> {
            Vec::new()
        }
        fn tmdb_api_key(&self) -> Option<String> {
            None
        }
        fn metadata_language(&self) -> String {
            "en".into()
        }
        fn get_service(
            &self,
            _type_id: std::any::TypeId,
        ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
            None
        }
    }

    #[test]
    fn client_offline_errors() {
        let c = AcquisitionSearchClient::new(Arc::new(|| None));
        assert!(c.interactive_search(&MockHost, "req-1").is_err());
        assert!(c.grab(&MockHost, "req-1", "guid-1", "idx-1").is_err());
    }
}
