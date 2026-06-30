//! Background job: populate intro/credits markers. Per season it **always tries
//! both** sources and reconciles them:
//!   * **chapters** — embedded chapter titles (cheap ffprobe).
//!   * **fingerprint** — decode each episode's start/end audio, align the season
//!     pairwise (`rusty-chromaprint`), keep the shared intro / credits run.
//! Embedded chapters win when present (human-authored); fingerprint fills the gaps
//! and cross-checks. The heavy decode runs on a bounded thread pool for speed.
//!
//! Modes (`introDetection` setting): `off` (skip) · `chapters` (chapter pass only)
//! · `fingerprint` (both + compare, the powerful default).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use anyhow::Result;

use super::fingerprint::{self, abs_ms, WindowFp};
use crate::db::{self, Pool};
use crate::infra::probe::{markers_from_chapters, probe_file};
use crate::model::{MarkerKind, MediaItem, Season};
use crate::services::jobs::JobContext;

/// Seconds of audio fingerprinted from the start (intro) and end (credits).
const INTRO_WINDOW_S: u32 = 240;
const CREDITS_WINDOW_S: u32 = 360;
const INTRO_REGION_END_S: f32 = 150.0;
const MIN_INTRO_S: f32 = 10.0;
const MIN_CREDITS_S: f32 = 10.0;
/// Upper bound on concurrent ffmpeg/ffprobe decode workers. Deliberately low:
/// fingerprinting streams whole audio tracks off disk, which competes with live
/// playback IO. We also pause entirely while anyone is watching (see
/// [`wait_while_idle`]), so this only bounds the brief in-flight contention when a
/// stream starts mid-decode. Playback is always the priority.
const MAX_WORKERS: usize = 3;
/// Poll interval while paused for active playback.
const PAUSE_POLL_S: u64 = 4;

/// Per-episode analysis result from the parallel decode pass.
struct EpData {
    chapters: Vec<(MarkerKind, u64, u64)>,
    start_fp: Option<WindowFp>,
    end_fp: Option<WindowFp>,
}

pub fn run(ctx: &JobContext) -> Result<()> {
    let mode = ctx.state.settings.get_str("introDetection", "chapters");
    if mode == "off" {
        ctx.info("introDetection = 'off' — nothing to do");
        return Ok(());
    }
    let fingerprinting = mode == "fingerprint";
    let ffprobe = ctx.state.ffprobe_available;
    // Few workers, and never more than a quarter of the cores — fingerprinting
    // must stay out of live playback's way.
    let workers = (thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 4)
        .clamp(1, MAX_WORKERS);
    let pool = &ctx.state.db;
    let shows = db::list_shows(pool, None)?;
    let total: usize = shows.iter().map(|s| s.episode_count as usize).sum();
    ctx.info(format!(
        "{} pass over {} show(s) / {total} episode(s), {workers} workers",
        if fingerprinting { "chapter + fingerprint" } else { "chapter" },
        shows.len()
    ));

    let mut done = 0usize;
    let mut written = 0usize;
    let show_count = shows.len();
    for (idx, show) in shows.iter().enumerate() {
        if ctx.cancelled() {
            ctx.info("cancelled");
            break;
        }
        let Some(detail) = db::get_show(pool, &show.id)? else {
            continue;
        };
        ctx.info(format!(
            "[{}/{}] {} — {} season(s), {} episode(s)",
            idx + 1,
            show_count,
            show.title,
            detail.seasons.len(),
            show.episode_count
        ));
        for season in &detail.seasons {
            if ctx.cancelled() {
                break;
            }
            written +=
                process_season(ctx, pool, season, fingerprinting, ffprobe, workers, &mut done, total)?;
        }
    }
    ctx.info(format!("done — wrote {written} marker(s) across {show_count} show(s)"));
    Ok(())
}

fn process_season(
    ctx: &JobContext,
    pool: &Pool,
    season: &Season,
    fingerprinting: bool,
    ffprobe: bool,
    workers: usize,
    done: &mut usize,
    total: usize,
) -> Result<usize> {
    let eps: Vec<&MediaItem> = season
        .episodes
        .iter()
        .filter(|e| e.abs_path.is_some() && e.duration_ms.unwrap_or(0) > 0)
        .collect();
    if eps.is_empty() {
        *done += season.episodes.len();
        ctx.progress(*done, total);
        return Ok(0);
    }
    let do_fp = fingerprinting && eps.len() >= 2;
    if do_fp {
        ctx.info(format!("  S{} — fingerprinting {} episode(s)…", season.number, eps.len()));
    }

    // Parallel decode: chapters (always) + audio fingerprints (when fingerprinting).
    let data = parallel_decode(&eps, do_fp, ffprobe, workers, ctx);
    if ctx.cancelled() {
        *done += season.episodes.len();
        ctx.progress(*done, total);
        return Ok(0);
    }

    let support = (eps.len() / 3).max(1);
    let mut written = 0usize;
    for (i, e) in eps.iter().enumerate() {
        for kind in [MarkerKind::Intro, MarkerKind::Credits] {
            let chap = data[i].chapters.iter().find(|(k, _, _)| *k == kind).map(|(_, s, en)| (*s, *en));
            let fp = if do_fp { align(&data, i, kind, support, e) } else { None };
            if let Some((start, end, source)) = reconcile(kind, chap, fp, ctx, e) {
                db::set_marker(pool, &e.id, kind, start, end, source)?;
                written += 1;
            }
        }
    }
    *done += season.episodes.len();
    ctx.progress(*done, total);
    Ok(written)
}

/// Decode every episode concurrently on a bounded worker pool: read chapters via
/// ffprobe and (when `do_fp`) fingerprint the start + end audio windows.
fn parallel_decode(
    eps: &[&MediaItem],
    do_fp: bool,
    ffprobe: bool,
    workers: usize,
    ctx: &JobContext,
) -> Vec<EpData> {
    let slots: Vec<Mutex<Option<EpData>>> = (0..eps.len()).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let paused = AtomicBool::new(false);
    thread::scope(|scope| {
        for _ in 0..workers.min(eps.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= eps.len() || ctx.cancelled() {
                    break;
                }
                // Playback is the priority: block before each heavy decode while
                // anyone is watching, so fingerprinting never competes for disk IO.
                wait_while_idle(ctx, &paused);
                if ctx.cancelled() {
                    break;
                }
                *slots[i].lock().unwrap() = Some(decode_one(ctx, &paused, eps[i], do_fp, ffprobe));
            });
        }
    });
    slots
        .into_iter()
        .map(|m| m.into_inner().unwrap().unwrap_or(EpData { chapters: Vec::new(), start_fp: None, end_fp: None }))
        .collect()
}

/// Block while any playback session is live, so the job yields all disk/CPU to
/// streaming. Logs the pause/resume transition exactly once (CAS on `paused`).
fn wait_while_idle(ctx: &JobContext, paused: &AtomicBool) {
    loop {
        if ctx.cancelled() {
            return;
        }
        if ctx.state.playback.list().is_empty() {
            if paused.swap(false, Ordering::Relaxed) {
                ctx.info("  ▶ playback ended — resuming");
            }
            return;
        }
        if !paused.swap(true, Ordering::Relaxed) {
            ctx.info("  ⏸ playback active — pausing (playback has priority)");
        }
        thread::sleep(Duration::from_secs(PAUSE_POLL_S));
    }
}

fn decode_one(
    ctx: &JobContext,
    paused: &AtomicBool,
    e: &MediaItem,
    do_fp: bool,
    ffprobe: bool,
) -> EpData {
    let path = std::path::Path::new(e.abs_path.as_ref().unwrap());
    let result = probe_file(path, ffprobe);
    let duration = result.duration_ms.or(e.duration_ms);
    let chapters = markers_from_chapters(&result.chapters, duration);
    let (start_fp, end_fp) = if do_fp {
        let dur_s = e.duration_ms.unwrap() as f64 / 1000.0;
        let start = fingerprint::fingerprint_window(path, INTRO_WINDOW_S, false, dur_s).ok();
        // Re-check between the two heavy decodes — a stream may have just started.
        wait_while_idle(ctx, paused);
        let end = fingerprint::fingerprint_window(path, CREDITS_WINDOW_S, true, dur_s).ok();
        (start, end)
    } else {
        (None, None)
    };
    EpData { chapters, start_fp, end_fp }
}

/// The fingerprint-derived range for `kind` on episode `i`, via pairwise alignment
/// + season consensus. Returns absolute ms; credits run to the episode end.
fn align(data: &[EpData], i: usize, kind: MarkerKind, support: usize, e: &MediaItem) -> Option<(u64, u64)> {
    let (pick, region, min_len): (fn(&EpData) -> &Option<WindowFp>, (f32, f32), f32) = match kind {
        MarkerKind::Intro => (|d| &d.start_fp, (0.0, INTRO_REGION_END_S), MIN_INTRO_S),
        MarkerKind::Credits => {
            (|d| &d.end_fp, (0.0, CREDITS_WINDOW_S as f32 - MIN_CREDITS_S), MIN_CREDITS_S)
        }
    };
    let fp = pick(&data[i]).as_ref()?;
    let mut ranges = Vec::new();
    for (j, d) in data.iter().enumerate() {
        if i == j {
            continue;
        }
        if let Some(other) = pick(d) {
            if let Some(r) = fingerprint::matched_range(&fp.data, &other.data, region, min_len) {
                ranges.push(r);
            }
        }
    }
    let (s, en) = fingerprint::consensus(ranges, support)?;
    let start = abs_ms(fp.window_start_s, s);
    // Credits extend to the end of the file; intro keeps its matched range.
    match kind {
        MarkerKind::Credits => Some((start, e.duration_ms.unwrap_or_else(|| abs_ms(fp.window_start_s, en)))),
        MarkerKind::Intro => Some((start, abs_ms(fp.window_start_s, en))),
    }
}

/// Pick the marker to store from the two sources and log the comparison. Chapters
/// win when present (authoritative); fingerprint fills gaps. Returns the stored
/// `(start, end, source)` or `None` if neither source found the segment.
fn reconcile(
    kind: MarkerKind,
    chap: Option<(u64, u64)>,
    fp: Option<(u64, u64)>,
    ctx: &JobContext,
    e: &MediaItem,
) -> Option<(u64, u64, &'static str)> {
    let k = kind_label(kind);
    let lbl = ep_label(e);
    match (chap, fp) {
        (Some(c), Some(f)) => {
            let delta = c.0.abs_diff(f.0) / 1000;
            ctx.info(format!(
                "  {lbl} {k}: chapters {} vs fingerprint {} (Δ{delta}s) → chapters",
                rng(c),
                rng(f)
            ));
            Some((c.0, c.1, "chapters"))
        }
        (Some(c), None) => {
            ctx.info(format!("  + {k} {lbl} {} (chapters)", rng(c)));
            Some((c.0, c.1, "chapters"))
        }
        (None, Some(f)) => {
            ctx.info(format!("  + {k} {lbl} {} (fingerprint)", rng(f)));
            Some((f.0, f.1, "fingerprint"))
        }
        (None, None) => None,
    }
}

fn kind_label(k: MarkerKind) -> &'static str {
    match k {
        MarkerKind::Intro => "intro",
        MarkerKind::Credits => "credits",
    }
}

fn fmt_ms(ms: u64) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn rng((a, b): (u64, u64)) -> String {
    format!("{}–{}", fmt_ms(a), fmt_ms(b))
}

fn ep_label(e: &MediaItem) -> String {
    match (e.season, e.episode) {
        (Some(s), Some(ep)) => format!("S{s}E{ep}"),
        _ => e.title.clone(),
    }
}
