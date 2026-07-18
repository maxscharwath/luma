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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AudioStream, Kind, MediaFile, VideoStream};

    fn base_item() -> MediaItem {
        MediaItem {
            id: "it1".into(),
            title: "The Film".into(),
            kind: Kind::Movie,
            year: Some(2020),
            duration_ms: Some(7_200_000),
            container: "mkv".into(),
            video: None,
            audio: None,
            audio_tracks: Vec::new(),
            subtitles: Vec::new(),
            library: "lib".into(),
            show_id: None,
            show_title: None,
            season: None,
            episode: None,
            episode_end: None,
            episode_title: None,
            rel_path: None,
            added_at: "t".into(),
            metadata: None,
            abs_path: None,
            files: Vec::new(),
            default_file_id: None,
            markers: Vec::new(),
            audio_analysis: None,
        }
    }

    fn vid(codec: &str, width: u32, hdr: bool) -> VideoStream {
        VideoStream { codec: codec.into(), width: Some(width), height: Some(width * 9 / 16), hdr, bit_depth: Some(10) }
    }

    fn aud(codec: &str, ch: u32) -> AudioStream {
        AudioStream { index: 0, codec: codec.into(), channels: Some(ch), language: Some("eng".into()), title: None, default: true }
    }

    #[test]
    fn snapshot_builds_video_and_audio_labels() {
        let mut it = base_item();
        it.video = Some(vid("hevc", 3840, true));
        it.audio = Some(aud("eac3", 6));
        let snap = snapshot(&it);
        assert_eq!(snap.title, "The Film");
        assert_eq!(snap.year, Some(2020));
        assert_eq!(snap.kind, "movie");
        assert_eq!(snap.video_label, "4K HDR · H.265");
        assert_eq!(snap.audio_label, "5.1 · EAC3");
    }

    #[test]
    fn snapshot_no_streams_shows_dashes() {
        let snap = snapshot(&base_item());
        assert_eq!(snap.video_label, "-");
        assert_eq!(snap.audio_label, "-");
    }

    #[test]
    fn snapshot_non_hdr_video_label() {
        let mut it = base_item();
        it.video = Some(vid("h264", 1920, false));
        assert_eq!(snapshot(&it).video_label, "1080p · H.264");
    }

    #[test]
    fn resolution_label_boundaries() {
        assert_eq!(resolution_label(Some(3840)), "4K");
        assert_eq!(resolution_label(Some(3000)), "4K");
        assert_eq!(resolution_label(Some(1920)), "1080p");
        assert_eq!(resolution_label(Some(1280)), "720p");
        assert_eq!(resolution_label(Some(1200)), "720p");
        assert_eq!(resolution_label(Some(640)), "SD");
        assert_eq!(resolution_label(Some(0)), "-");
        assert_eq!(resolution_label(None), "-");
    }

    #[test]
    fn video_codec_label_maps_known_and_uppercases_other() {
        assert_eq!(video_codec_label("hevc"), "H.265");
        assert_eq!(video_codec_label("H265"), "H.265");
        assert_eq!(video_codec_label("h264"), "H.264");
        assert_eq!(video_codec_label("avc"), "H.264");
        assert_eq!(video_codec_label("av1"), "AV1");
        assert_eq!(video_codec_label("vp9"), "VP9");
        assert_eq!(video_codec_label("mpeg2"), "MPEG2");
    }

    #[test]
    fn channels_label_maps() {
        assert_eq!(channels_label(Some(8)), "7.1");
        assert_eq!(channels_label(Some(7)), "6.1");
        assert_eq!(channels_label(Some(6)), "5.1");
        assert_eq!(channels_label(Some(2)), "Stéréo");
        assert_eq!(channels_label(Some(1)), "Mono");
        assert_eq!(channels_label(Some(3)), "Audio");
        assert_eq!(channels_label(None), "Audio");
    }

    #[test]
    fn bitrate_uses_default_file_size_over_duration() {
        let mut it = base_item();
        it.duration_ms = Some(1000); // 1 second
        it.files = vec![MediaFile {
            id: "f1".into(),
            rel_path: None,
            container: "mkv".into(),
            duration_ms: Some(1000),
            video: None,
            audio: None,
            audio_tracks: Vec::new(),
            subtitles: Vec::new(),
            size: Some(1_000_000), // 1 MB -> 8 Mbit over 1 s = 8 Mb/s
            edition: None,
            probed: true,
            abs_path: None,
        }];
        it.default_file_id = Some("f1".into());
        assert_eq!(bitrate_mbps(&it), 8.0);
    }

    #[test]
    fn bitrate_zero_when_no_duration_or_no_size() {
        let mut it = base_item();
        it.duration_ms = Some(0);
        assert_eq!(bitrate_mbps(&it), 0.0);
        // Has duration but no file size.
        it.duration_ms = Some(1000);
        it.files = Vec::new();
        assert_eq!(bitrate_mbps(&it), 0.0);
    }
}
