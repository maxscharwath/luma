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
