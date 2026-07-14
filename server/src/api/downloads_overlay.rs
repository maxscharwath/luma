//! The live-grab overlay for request / discover list views: derives the
//! transient `downloading` / `importing` status + progress for a request from the
//! shared `downloads` table (the relationship, so nothing goes stale when a
//! torrent fails or is deleted).
//!
//! Read directly here rather than through the Downloads module, so the core names
//! no module crate; the `downloads` table is created by that module's migrations
//! (in the shared DB), so this tolerates its absence -- when the Downloads module
//! isn't installed, the overlay is simply empty.

use std::collections::HashMap;

use rusqlite::Connection;

/// A request with a live grab. `importing` = a completed grab is being imported
/// (vs still downloading); `progress` = mean progress (0..1) across its rows.
#[derive(Debug, Clone)]
pub struct ActiveDownload {
    pub request_id: String,
    pub importing: bool,
    pub progress: f64,
}

/// The active-download rows keyed by request id. Empty (not an error) when the
/// `downloads` table is absent -- i.e. the Downloads module isn't installed.
pub fn active_downloads(conn: &Connection) -> HashMap<String, ActiveDownload> {
    let mut out = HashMap::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT request_id, MAX(status = 'completed'), AVG(progress) FROM downloads \
         WHERE request_id IS NOT NULL \
           AND status IN ('queued', 'downloading', 'seeding', 'completed', 'paused') \
         GROUP BY request_id",
    ) else {
        return out;
    };
    let rows = stmt.query_map([], |r| {
        Ok(ActiveDownload {
            request_id: r.get::<_, String>(0)?,
            importing: r.get::<_, i64>(1)? != 0,
            progress: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
        })
    });
    if let Ok(rows) = rows {
        for a in rows.flatten() {
            out.insert(a.request_id.clone(), a);
        }
    }
    out
}
