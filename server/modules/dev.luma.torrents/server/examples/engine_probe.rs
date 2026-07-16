//! Swarm-connectivity probe for the embedded engine: starts a librqbit session
//! with the exact options the app uses (see `rqbit.rs`) and adds one torrent,
//! printing peer stats every 5s. Diagnoses "stuck at 0%" without the server.
//!
//!   cargo run -p luma-torrent --example engine_probe --features rqbit -- \
//!     <file.torrent> <work_dir> [socks5://host:port]

use std::time::Duration;

use librqbit::limits::LimitsConfig;
use librqbit::{AddTorrent, AddTorrentOptions, Session, SessionOptions, SessionPersistenceConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,librqbit=debug".into()),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let torrent = args.next().expect("usage: engine_probe <file.torrent> <work_dir> [socks_url]");
    let work_dir = std::path::PathBuf::from(args.next().expect("work_dir required"));
    let socks = args.next();

    let session_dir = work_dir.join("session");
    let download_dir = work_dir.join("downloads");
    std::fs::create_dir_all(&session_dir)?;
    std::fs::create_dir_all(&download_dir)?;

    let opts = SessionOptions {
        persistence: Some(SessionPersistenceConfig::Json { folder: Some(session_dir) }),
        fastresume: true,
        socks_proxy_url: socks.clone(),
        listen_port_range: None,
        ratelimits: LimitsConfig { download_bps: None, upload_bps: None },
        disable_dht_persistence: true,
        enable_upnp_port_forwarding: false,
        ..Default::default()
    };
    println!("starting session (socks={socks:?})");
    let session = Session::new_with_opts(download_dir, opts).await?;

    let bytes = std::fs::read(&torrent)?;
    let added = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions { overwrite: true, ..Default::default() }),
        )
        .await?;
    let handle = added.into_handle().expect("torrent handle");
    println!("added: {} ({})", handle.name().unwrap_or_default(), handle.info_hash().as_string());

    if std::env::var("REANNOUNCE_TEST").is_ok() {
        // Same cycle RqbitClient::reannounce runs: pause -> unpause must
        // rebuild the live task and re-announce.
        tokio::time::sleep(Duration::from_secs(3)).await;
        session.pause(&handle).await?;
        session.unpause(&handle).await?;
        println!("reannounce cycle (pause->unpause) OK");
    }

    for tick in 1..=24 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let stats = handle.stats();
        let (live, seen, queued, dead, speed) = stats
            .live
            .as_ref()
            .map(|l| {
                let p = &l.snapshot.peer_stats;
                (p.live, p.seen, p.queued, p.dead, l.download_speed.mbps)
            })
            .unwrap_or_default();
        println!(
            "tick {tick:2}: state={:?} progress={}/{} peers live={live} seen={seen} queued={queued} dead={dead} down={speed:.2} MiB/s error={:?}",
            stats.state, stats.progress_bytes, stats.total_bytes, stats.error
        );
        if stats.progress_bytes > 0 {
            println!("DATA FLOWING - engine layer is healthy");
            break;
        }
    }
    session.stop().await;
    Ok(())
}
