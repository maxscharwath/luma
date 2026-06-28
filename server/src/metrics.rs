//! System metrics sampler — the data behind the dashboard's Débit / Processeur /
//! RAM charts and the Stockage page.
//!
//! A background task samples CPU + RAM via `sysinfo` every [`SAMPLE_INTERVAL`]
//! and keeps a rolling ring buffer (~[`HISTORY`] samples ≈ the design's 2-minute
//! window). Bandwidth is derived from the live playback registry (sum of stream
//! bitrates, split LAN vs WAN) rather than instrumenting the byte stream — it's
//! the throughput actually being delivered. Disk usage is read on demand for the
//! storage page.

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Disks, System};

use crate::playback::Registry;
use crate::process_started;

/// Sampling cadence. Matches the dashboard's live feel; well above sysinfo's
/// minimum CPU refresh interval.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(1500);
/// Ring-buffer length (≈ 3 min at 1.5s/sample, covering the design's 2m window).
const HISTORY: usize = 120;

/// Time-series history (oldest → newest). Percentages are 0..100.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub cpu_luma: Vec<f32>,
    pub cpu_system: Vec<f32>,
    pub ram_luma: Vec<f32>,
    pub ram_system: Vec<f32>,
    /// Bandwidth in Mb/s.
    pub bw_local: Vec<f64>,
    pub bw_remote: Vec<f64>,
}

/// A point-in-time metrics snapshot plus the recent history series.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub cpu_luma: f32,
    pub cpu_system: f32,
    pub ram_luma_bytes: u64,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    pub bw_local_mbps: f64,
    pub bw_remote_mbps: f64,
    pub uptime_secs: u64,
    pub series: Series,
}

#[derive(Default)]
struct Hist {
    cpu_luma: VecDeque<f32>,
    cpu_system: VecDeque<f32>,
    ram_luma: VecDeque<f32>,
    ram_system: VecDeque<f32>,
    bw_local: VecDeque<f64>,
    bw_remote: VecDeque<f64>,
    cur: Snapshot,
}

/// Shared, cheap-to-clone handle to the rolling metrics history.
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<RwLock<Hist>>,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            inner: Arc::new(RwLock::new(Hist::default())),
        }
    }

    /// Current values + the recent history series, for `GET /api/admin/metrics`.
    pub fn snapshot(&self) -> Snapshot {
        let h = self.inner.read().unwrap();
        let mut snap = h.cur.clone();
        snap.uptime_secs = process_started().elapsed().as_secs();
        snap.series = Series {
            cpu_luma: h.cpu_luma.iter().copied().collect(),
            cpu_system: h.cpu_system.iter().copied().collect(),
            ram_luma: h.ram_luma.iter().copied().collect(),
            ram_system: h.ram_system.iter().copied().collect(),
            bw_local: h.bw_local.iter().copied().collect(),
            bw_remote: h.bw_remote.iter().copied().collect(),
        };
        snap
    }

    fn push(&self, snap: Snapshot, ram_luma_pct: f32, ram_sys_pct: f32) {
        let mut h = self.inner.write().unwrap();
        push_cap(&mut h.cpu_luma, snap.cpu_luma);
        push_cap(&mut h.cpu_system, snap.cpu_system);
        push_cap(&mut h.ram_luma, ram_luma_pct);
        push_cap(&mut h.ram_system, ram_sys_pct);
        push_cap(&mut h.bw_local, snap.bw_local_mbps);
        push_cap(&mut h.bw_remote, snap.bw_remote_mbps);
        h.cur = snap;
    }

    /// Spawn the sampler. `registry` feeds the bandwidth series.
    pub fn spawn_sampler(&self, registry: Registry) {
        let metrics = self.clone();
        // sysinfo work is blocking-ish but cheap; a dedicated OS thread keeps it
        // off the async runtime and lets us sleep precisely.
        std::thread::spawn(move || {
            let pid = sysinfo::get_current_pid().ok();
            let mut sys = System::new();
            let cpus = num_cpus_safe(&mut sys);
            loop {
                sys.refresh_cpu();
                sys.refresh_memory();
                if let Some(pid) = pid {
                    sys.refresh_process(pid);
                }

                let cpu_system = sys.global_cpu_info().cpu_usage();
                let (cpu_luma, ram_luma_bytes) = pid
                    .and_then(|p| sys.process(p))
                    .map(|proc| ((proc.cpu_usage() / cpus).min(100.0), proc.memory()))
                    .unwrap_or((0.0, 0));

                let ram_total = sys.total_memory().max(1);
                let ram_used = sys.used_memory();
                let ram_sys_pct = (ram_used as f32 / ram_total as f32) * 100.0;
                let ram_luma_pct = (ram_luma_bytes as f32 / ram_total as f32) * 100.0;

                let (bw_local, bw_remote) = bandwidth(&registry);

                metrics.push(
                    Snapshot {
                        cpu_luma,
                        cpu_system,
                        ram_luma_bytes,
                        ram_used_bytes: ram_used,
                        ram_total_bytes: ram_total,
                        bw_local_mbps: bw_local,
                        bw_remote_mbps: bw_remote,
                        uptime_secs: 0,
                        series: Series::default(),
                    },
                    ram_luma_pct,
                    ram_sys_pct,
                );

                std::thread::sleep(SAMPLE_INTERVAL);
            }
        });
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Sum the live sessions' bitrates (Mb/s), split by network class.
fn bandwidth(registry: &Registry) -> (f64, f64) {
    let mut local = 0.0;
    let mut remote = 0.0;
    for s in registry.list() {
        // Paused streams aren't pulling bytes.
        if s.state != "playing" {
            continue;
        }
        if s.network == "LAN" {
            local += s.bitrate;
        } else {
            remote += s.bitrate;
        }
    }
    (local, remote)
}

fn push_cap<T>(buf: &mut VecDeque<T>, v: T) {
    if buf.len() >= HISTORY {
        buf.pop_front();
    }
    buf.push_back(v);
}

fn num_cpus_safe(sys: &mut System) -> f32 {
    sys.refresh_cpu();
    let n = sys.cpus().len();
    if n == 0 {
        1.0
    } else {
        n as f32
    }
}

// ----- disks (storage page) ---------------------------------------------------

/// One mounted volume's usage.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub fs: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

/// Read all mounted volumes (deduped by mount point), largest first.
pub fn read_disks() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<DiskInfo> = Vec::new();
    for d in disks.list() {
        let mount = d.mount_point().to_string_lossy().to_string();
        // Skip pseudo/duplicate mounts and anything with no capacity.
        if d.total_space() == 0 || !seen.insert(mount.clone()) {
            continue;
        }
        let total = d.total_space();
        let avail = d.available_space();
        out.push(DiskInfo {
            name: d.name().to_string_lossy().to_string(),
            mount,
            fs: d.file_system().to_string_lossy().to_string(),
            total_bytes: total,
            used_bytes: total.saturating_sub(avail),
            available_bytes: avail,
        });
    }
    out.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    out
}
