//! The downloads monitor: a resident tokio task (the HLS-reaper pattern, NOT a
//! cron job: the scheduler ticks at 30s, far too coarse for live progress).
//! Fast ticks while anything is active, slow idle ticks otherwise. Each tick
//! polls the engines on a blocking thread, mirrors progress into the ledger,
//! publishes `download.progress` frames, and flips finished torrents to
//! `completed` (chaining the import job).

use std::sync::Arc;
use std::time::Duration;

use crate::db;
use crate::infra::events::ServerEvent;
use crate::services::jobs::{now_ms, JobKey};
use crate::state::SharedState;

use super::DownloadManager;

const ACTIVE_TICK: Duration = Duration::from_secs(5);
const IDLE_TICK: Duration = Duration::from_secs(30);
const VPN_CHECK_EVERY: Duration = Duration::from_secs(60);

fn human_bytes(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

impl DownloadManager {
    /// Spawn the resident monitor. Call once from `main` next to the reapers.
    pub fn spawn_monitor(self: &Arc<Self>, state: SharedState) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut last_vpn_check = std::time::Instant::now() - VPN_CHECK_EVERY;
            loop {
                let vpn_due = last_vpn_check.elapsed() >= VPN_CHECK_EVERY;
                if vpn_due {
                    last_vpn_check = std::time::Instant::now();
                }
                let had_active = tokio::task::spawn_blocking({
                    let manager = manager.clone();
                    let state = state.clone();
                    move || {
                        if vpn_due {
                            let _ = manager.vpn_check(&state);
                        }
                        manager.tick(&state)
                    }
                })
                .await
                .unwrap_or(false);
                tokio::time::sleep(if had_active { ACTIVE_TICK } else { IDLE_TICK }).await;
            }
        });
    }

    /// One poll pass. Returns whether anything is still active (drives the
    /// tick cadence).
    fn tick(&self, state: &SharedState) -> bool {
        let rows = match state.db.get().and_then(|c| Ok(db::active_downloads(&c)?)) {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "downloads monitor: ledger read failed");
                return false;
            }
        };
        if rows.is_empty() {
            return false;
        }

        let mut completed_any = false;
        for row in &rows {
            // Still being added in the background (grab enqueues, then activate()
            // fills the ref). Nothing to poll yet.
            if row.client_ref.is_empty() {
                continue;
            }
            let client = match state.db.get().and_then(|c| Ok(db::get_download_client(&c, &row.client_id)?)) {
                Ok(Some(c)) => c,
                _ => {
                    let _ = db::set_download_status(&state.db, &row.id, "failed", Some("download client removed"));
                    continue;
                }
            };
            let engine = match self.engine_for(&client) {
                Ok(e) => e,
                Err(e) => {
                    // Engine offline (external daemon down, embedded not
                    // started): leave the row as-is and retry next tick.
                    tracing::debug!(id = %row.id, error = %format!("{e:#}"), "engine unavailable");
                    continue;
                }
            };
            match engine.status(&row.client_ref) {
                Ok(Some(status)) => {
                    // Visibility for "stuck at 0%": show what the swarm looks
                    // like. peers=0 & seen=0 -> the tracker returned nothing
                    // (dead torrent / announce blocked). seen>0 & peers=0 ->
                    // discovered but can't connect (firewall / no port-forward).
                    tracing::info!(
                        release = %row.release_title,
                        state = ?status.state,
                        progress = format!("{:.1}%", status.progress * 100.0),
                        peers = status.peers,
                        peers_seen = status.peers_seen,
                        down = format!("{}/s", human_bytes(status.down_bps)),
                        "download tick"
                    );
                    let finished = status.progress >= 1.0
                        || matches!(status.state, luma_torrents::TorrentState::Completed | luma_torrents::TorrentState::Seeding);
                    let new_status = if finished {
                        "completed"
                    } else {
                        match status.state {
                            luma_torrents::TorrentState::Paused => "paused",
                            luma_torrents::TorrentState::Queued => "queued",
                            // Error included: transient tracker/disk errors
                            // recover, so keep polling with the error visible.
                            _ => "downloading",
                        }
                    };
                    if finished {
                        // save_path may only be known now (external clients).
                        let _ = db::update_download_progress(
                            &state.db,
                            &row.id,
                            "completed",
                            1.0,
                            status.save_path.as_deref(),
                            None,
                        );
                        let _ = db::mark_download_completed(&state.db, &row.id, now_ms());
                    } else {
                        let _ = db::update_download_progress(
                            &state.db,
                            &row.id,
                            new_status,
                            status.progress,
                            status.save_path.as_deref(),
                            status.error.as_deref(),
                        );
                    }
                    state.events.publish(ServerEvent::DownloadProgress {
                        id: row.id.clone(),
                        request_id: row.request_id.clone(),
                        progress: status.progress,
                        down_bps: status.down_bps,
                        up_bps: status.up_bps,
                        peers: status.peers,
                        peers_seen: status.peers_seen,
                        state: new_status.to_string(),
                    });
                    if finished {
                        state.events.publish(ServerEvent::DownloadCompleted {
                            id: row.id.clone(),
                            title: row.release_title.clone(),
                        });
                        completed_any = true;
                    }
                }
                Ok(None) => {
                    // The torrent vanished from the engine (user removed it
                    // there, or a session reset): the grab failed.
                    let _ = db::set_download_status(
                        &state.db,
                        &row.id,
                        "failed",
                        Some("torrent disappeared from the download client"),
                    );
                }
                Err(e) => {
                    tracing::debug!(id = %row.id, error = %format!("{e:#}"), "status poll failed");
                }
            }
        }

        if completed_any {
            // Import runs as a tracked job so its work shows in the console.
            let _ = state.jobs.trigger(state.clone(), JobKey("acquisition.import"), "download-complete");
        }
        true
    }
}
