//! Admin-facing indexer orchestration: per-indexer capability caching, the
//! native-engine session cache, and the definition store. Moved out of the app's
//! acquisition service so the Indexers module owns its vertical; it reaches the
//! app only through the [`HostCtx`] seam (DB pool, data dir, settings).
//!
//! The interactive/automatic search DISPATCH (`search_indexer`) stays in the app
//! (it belongs to the broader acquisition pipeline); it calls into these.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::db::IndexerRow;
use kroma_module_sdk::host::HostCtx;
use kroma_module_sdk::primitives::now_ms;
use kroma_module_sdk::ports::{Caps, IndexerEndpoint};

use crate::store::DefinitionStore;
use crate::{Caps as EngineCaps, IndexerConfig, Session};

/// `kind` value for a native-engine indexer row.
pub use kroma_module_sdk::ports::KIND_BUILTIN;

pub fn endpoint_of(row: &IndexerRow) -> IndexerEndpoint {
    IndexerEndpoint {
        url: row.url.clone(),
        api_key: row.api_key.clone(),
        categories: row.categories.clone(),
    }
}

/// Process-wide `t=caps` cache: capabilities are static per indexer, and every
/// search would otherwise pay an extra round-trip per indexer. Keyed by indexer
/// id + url (so re-pointing an indexer refreshes).
static CAPS_CACHE: Mutex<Option<HashMap<String, Caps>>> = Mutex::new(None);

pub fn indexer_caps(host: &dyn HostCtx, row: &IndexerRow) -> anyhow::Result<Caps> {
    let key = format!("{}|{}", row.id, row.url);
    if let Some(caps) = CAPS_CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)).cloned() {
        return Ok(caps);
    }
    let result = kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::TorznabPort>(host)
        .ok_or_else(|| anyhow::anyhow!("torznab search engine unavailable"))
        .and_then(|p| p.caps(&endpoint_of(row)));
    match &result {
        Ok(_) => {
            let _ = crate::db::note_indexer_result(host.db(), &row.id, true, None, now_ms());
        }
        Err(e) => {
            let _ = crate::db::note_indexer_result(
                host.db(),
                &row.id,
                false,
                Some(&format!("{e:#}")),
                now_ms(),
            );
        }
    }
    let caps = result?;
    CAPS_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, caps.clone());
    Ok(caps)
}

/// Drop a cached capability entry (config changed / manual test forces a fresh
/// probe), plus any cached built-in session for that indexer.
pub fn invalidate_caps(indexer_id: &str) {
    let prefix = format!("{indexer_id}|");
    if let Some(map) = CAPS_CACHE.lock().unwrap().as_mut() {
        map.retain(|k, _| !k.starts_with(&prefix));
    }
    if let Some(map) = SESSION_CACHE.lock().unwrap().as_mut() {
        map.retain(|k, _| !k.starts_with(&prefix));
    }
}

/// The runtime definition cache (lives under the data dir).
pub fn definition_store(host: &dyn HostCtx) -> DefinitionStore {
    DefinitionStore::new(host.data_dir())
}

/// Process-wide cache of built-in [`Session`]s, keyed so a config change
/// (url / settings / VPN / FlareSolverr) yields a fresh session. Reusing one
/// session across a sweep is what makes `requestDelay` throttling + the login
/// cookie jar persist across the dozens of back-to-back requests a sweep fires.
static SESSION_CACHE: Mutex<Option<HashMap<String, Arc<Session>>>> = Mutex::new(None);

fn session_key(host: &dyn HostCtx, row: &IndexerRow) -> String {
    let vpn = host.setting_bool("acqIndexersUseVpn", false);
    let fs = host.setting_str("acqFlaresolverrUrl", "");
    format!("{}|{}|{}|{vpn}|{fs}", row.id, row.url, row.settings)
}

/// Get (or build + cache) the shared session for a `builtin` indexer row.
pub fn builtin_session(host: &dyn HostCtx, row: &IndexerRow) -> anyhow::Result<Arc<Session>> {
    let key = session_key(host, row);
    if let Some(s) = SESSION_CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)).cloned() {
        return Ok(s);
    }
    let session = Arc::new(build_builtin_session(host, row)?);
    SESSION_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, session.clone());
    Ok(session)
}

/// The VPN SOCKS5 URL search traffic should use, when the admin opted indexers
/// into the tunnel (`acqIndexersUseVpn`) AND a bridge is configured. Checks the
/// opt-in first so the WireGuard config isn't read on the common (off) path.
fn vpn_proxy_url(host: &dyn HostCtx) -> Option<String> {
    if host.setting_bool("acqIndexersUseVpn", false) {
        kroma_module_sdk::host::resolve_port::<dyn kroma_module_sdk::ports::VpnProxyPort>(host)
            .and_then(|p| p.proxy_url(host))
    } else {
        None
    }
}

/// Build a live [`Session`] for a `builtin` indexer row: load its definition,
/// seed config from the stored settings JSON + base link, and wire optional
/// VPN / FlareSolverr transport. Prefer [`builtin_session`] (cached) on hot paths.
pub fn build_builtin_session(host: &dyn HostCtx, row: &IndexerRow) -> anyhow::Result<Session> {
    let def_id = row
        .definition_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("built-in indexer '{}' has no definition", row.name))?;
    let def = definition_store(host).load(def_id)?;
    let settings: HashMap<String, String> = serde_json::from_str(&row.settings).unwrap_or_default();
    let cfg = IndexerConfig { base_url: row.url.clone(), settings };

    // Route search traffic through the VPN bridge only when the admin opted in
    // (a downed tunnel must not silently break search otherwise).
    let socks5 = vpn_proxy_url(host);
    let flaresolverr = {
        let url = host.setting_str("acqFlaresolverrUrl", "");
        (!url.trim().is_empty()).then(|| url.trim().to_string())
    };

    Ok(Session::new(host.data_dir(), &row.id, def, cfg, socks5, flaresolverr))
}

/// Capabilities for any indexer kind (native ones derive from the definition; no
/// network round-trip).
pub fn any_indexer_caps(host: &dyn HostCtx, row: &IndexerRow) -> anyhow::Result<Caps> {
    if row.kind == KIND_BUILTIN {
        let def = definition_store(host)
            .load(row.definition_id.as_deref().ok_or_else(|| anyhow::anyhow!("no definition"))?)?;
        let c = EngineCaps::from_definition(&def);
        Ok(Caps {
            search_tmdb: c.search_tmdb,
            search_imdb: c.search_imdb,
            tv_search_tmdb: c.tv_search_tmdb,
            server_title: c.server_title,
        })
    } else {
        indexer_caps(host, row)
    }
}
