//! Swarm-connectivity probe for the embedded engine: starts a librqbit session
//! with the exact options the app uses (see `rqbit.rs`) and adds one torrent,
//! printing peer stats every 5s. Diagnoses "stuck at 0%" without the server.
//!
//!   cargo run -p kroma-torrent --example engine_probe --features rqbit -- \
//!     <file.torrent> <work_dir> [socks5://host:port]

use std::time::Duration;

use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, ConnectionOptions, DhtSessionConfig, ListenerMode,
    ListenerOptions, Session, SessionOptions, SessionPersistenceConfig,
};

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
        connect: Some(ConnectionOptions { proxy_url: socks.clone(), ..Default::default() }),
        listen: Some(ListenerOptions {
            mode: ListenerMode::TcpOnly,
            listen_addr: (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
            enable_upnp_port_forwarding: false,
            ..Default::default()
        }),
        ratelimits: LimitsConfig { download_bps: None, upload_bps: None },
        dht: Some(DhtSessionConfig { bootstrap_addrs: None, port: None, persistence: None }),
        ..Default::default()
    };
    println!("starting session (socks={socks:?})");
    let session = Session::new_with_opts(download_dir, opts).await?;

    let bytes = std::fs::read(&torrent)?;
    // Mirror the real engine's add-time seed: announce over the bridge (curl)
    // and hand librqbit the peers as initial_peers.
    let seed = socks.as_deref().map(|s| kroma_torrent::announce_peers(&bytes, Some(s)));
    if let Some(p) = &seed {
        println!("add-time seed: {} peers ({} v6)", p.len(), p.iter().filter(|a| a.is_ipv6()).count());
    }
    let added = session
        .add_torrent(
            AddTorrent::from_bytes(bytes.clone()),
            Some(AddTorrentOptions {
                overwrite: true,
                initial_peers: seed.clone().filter(|p| !p.is_empty()),
                ..Default::default()
            }),
        )
        .await?;
    let handle = added.into_handle().expect("torrent handle");
    println!("added: {} ({})", handle.name().unwrap_or_default(), handle.info_hash().as_string());

    for tick in 1..=30 {
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
