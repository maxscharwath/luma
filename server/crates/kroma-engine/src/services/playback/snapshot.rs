//! Derive a display-ready snapshot (title + stream labels + bitrate) from a
//! catalog item, for a live session card. Pure formatting no state.

use crate::model::MediaItem;

/// Derived, display-ready snapshot of an item for a session card.
#[derive(Default)]
pub(super) struct Snapshot {
    pub(super) title: String,
    pub(super) year: Option<u32>,
    pub(super) kind: String,
    pub(super) show_title: Option<String>,
    pub(super) season: Option<u32>,
    pub(super) episode: Option<u32>,
    pub(super) video_label: String,
    pub(super) audio_label: String,
    pub(super) bitrate: f64,
}

pub(super) fn snapshot(item: &MediaItem) -> Snapshot {
    let video_label = item
        .video
        .as_ref()
        .map(|v| {
            let res = resolution_label(v.width);
            let codec = video_codec_label(&v.codec);
            if v.hdr {
                format!("{res} HDR · {codec}")
            } else {
                format!("{res} · {codec}")
            }
        })
        .unwrap_or_else(|| "-".into());

    let audio_label = item
        .audio
        .as_ref()
        .map(|a| {
            let ch = channels_label(a.channels);
            let codec = a.codec.to_uppercase();
            format!("{ch} · {codec}")
        })
        .unwrap_or_else(|| "-".into());

    Snapshot {
        title: item.title.clone(),
        year: item.year,
        kind: kind_str(&item.kind).to_string(),
        show_title: item.show_title.clone(),
        season: item.season,
        episode: item.episode,
        video_label,
        audio_label,
        bitrate: bitrate_mbps(item),
    }
}

fn kind_str(k: &crate::model::Kind) -> &'static str {
    match k {
        crate::model::Kind::Movie => "movie",
        crate::model::Kind::Episode => "episode",
        crate::model::Kind::Video => "video",
    }
}

fn resolution_label(width: Option<u32>) -> &'static str {
    match width.unwrap_or(0) {
        w if w >= 3000 => "4K",
        w if w >= 1900 => "1080p",
        w if w >= 1200 => "720p",
        w if w > 0 => "SD",
        _ => "-",
    }
}

fn video_codec_label(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "hevc" | "h265" => "H.265".into(),
        "h264" | "avc" => "H.264".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        other => other.to_uppercase(),
    }
}

fn channels_label(ch: Option<u32>) -> &'static str {
    match ch.unwrap_or(0) {
        8 => "7.1",
        7 => "6.1",
        6 => "5.1",
        2 => "Stéréo",
        1 => "Mono",
        _ => "Audio",
    }
}

/// Approx stream bitrate in Mb/s from the representative file size ÷ duration.
fn bitrate_mbps(item: &MediaItem) -> f64 {
    let dur_s = item.duration_ms.unwrap_or(0) as f64 / 1000.0;
    if dur_s <= 0.0 {
        return 0.0;
    }
    let size = item
        .default_file_id
        .as_ref()
        .and_then(|id| item.files.iter().find(|f| &f.id == id))
        .or_else(|| item.files.first())
        .and_then(|f| f.size)
        .unwrap_or(0) as f64;
    if size <= 0.0 {
        return 0.0;
    }
    let mbps = size * 8.0 / dur_s / 1_000_000.0;
    (mbps * 10.0).round() / 10.0
}
