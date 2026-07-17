//! Parse ffprobe's JSON output into our best-effort [`ProbeResult`] model:
//! stream selection, codec normalization, HDR/bit-depth heuristics.

use serde::Deserialize;

use crate::model::{AudioStream, SubtitleTrack, VideoStream};

use super::{Chapter, ProbeResult};

/// Build our model from raw ffprobe output.
pub(super) fn build_result(raw: FfprobeOutput) -> ProbeResult {
    // Every audio stream, in container order. The audio-relative index (0-based
    // among audio streams) is the position here exactly ffmpeg's `0:a:<n>`.
    let audio_tracks: Vec<AudioStream> = raw
        .streams
        .iter()
        .filter(|&s| s.codec_type.as_deref() == Some("audio"))
        .enumerate()
        .map(|(i, s)| build_audio(s, i as u32))
        .collect();
    let audio = audio_tracks.first().cloned();

    ProbeResult {
        duration_ms: raw.format.as_ref().and_then(|fmt| {
            fmt.duration
                .as_deref()
                .and_then(|d| d.parse::<f64>().ok())
                .map(|secs| (secs * 1000.0) as u64)
        }),
        // First real video stream, skipping embedded cover-art (mjpeg posters):
        // the cover-art test must stay in the predicate so a leading poster
        // stream doesn't win and null out the actual video.
        video: raw
            .streams
            .iter()
            .find(|&s| s.codec_type.as_deref() == Some("video") && !is_probably_cover_art(s))
            .map(build_video),
        audio,
        audio_tracks,
        subtitles: raw
            .streams
            .iter()
            .filter(|&s| s.codec_type.as_deref() == Some("subtitle"))
            .map(|s| SubtitleTrack {
                language: s.language(),
                codec: normalize_codec(s.codec_name.as_deref()),
            })
            .collect(),
        chapters: raw.chapters.iter().filter_map(build_chapter).collect(),
    }
}

/// Map a raw ffprobe chapter (start/end in seconds as strings) to a [`Chapter`].
/// Dropped if the times are unparseable or zero-length.
fn build_chapter(c: &FfChapter) -> Option<Chapter> {
    let start = c.start_time.as_deref().and_then(|s| s.parse::<f64>().ok())?;
    let end = c.end_time.as_deref().and_then(|s| s.parse::<f64>().ok())?;
    if end <= start {
        return None;
    }
    Some(Chapter {
        start_ms: (start * 1000.0) as u64,
        end_ms: (end * 1000.0) as u64,
        title: c.tags.as_ref().and_then(|t| t.title.clone()).filter(|t| !t.trim().is_empty()),
    })
}

fn is_probably_cover_art(stream: &FfStream) -> bool {
    matches!(stream.codec_name.as_deref(), Some("mjpeg") | Some("png"))
        && stream.width.unwrap_or(0) <= 1000
        && stream.height.unwrap_or(0) <= 1000
}

fn build_video(stream: &FfStream) -> VideoStream {
    let bit_depth = stream
        .bits_per_raw_sample
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .or_else(|| pixel_format_bit_depth(stream.pix_fmt.as_deref()));

    let hdr = is_hdr(stream, bit_depth);

    VideoStream {
        codec: normalize_codec(stream.codec_name.as_deref()),
        width: stream.width,
        height: stream.height,
        hdr,
        bit_depth,
    }
}

fn build_audio(stream: &FfStream, index: u32) -> AudioStream {
    AudioStream {
        index,
        codec: normalize_codec(stream.codec_name.as_deref()),
        channels: stream.channels,
        language: stream.language(),
        title: stream.title(),
        default: stream.disposition.as_ref().is_some_and(|d| d.default == Some(1)),
    }
}

/// HDR heuristic: PQ / HLG transfer, or 10-bit+ with a wide-gamut primary.
fn is_hdr(stream: &FfStream, bit_depth: Option<u32>) -> bool {
    let transfer = stream.color_transfer.as_deref().unwrap_or("");
    if matches!(transfer, "smpte2084" | "arib-std-b67") {
        return true;
    }
    let wide_gamut = matches!(
        stream.color_primaries.as_deref().unwrap_or(""),
        "bt2020"
    );
    bit_depth.map(|b| b >= 10).unwrap_or(false) && wide_gamut
}

/// Map common pixel formats to a bit depth when `bits_per_raw_sample` is absent.
fn pixel_format_bit_depth(pix_fmt: Option<&str>) -> Option<u32> {
    let pix_fmt = pix_fmt?;
    if pix_fmt.contains("p10") || pix_fmt.contains("10le") || pix_fmt.contains("10be") {
        Some(10)
    } else if pix_fmt.contains("p12") || pix_fmt.contains("12le") || pix_fmt.contains("12be") {
        Some(12)
    } else if !pix_fmt.is_empty() {
        Some(8)
    } else {
        None
    }
}

/// Normalize a codec name to the lowercase canonical form clients expect.
pub fn normalize_codec(name: Option<&str>) -> String {
    let raw = name.unwrap_or("unknown").to_ascii_lowercase();
    match raw.as_str() {
        "h265" | "hevc" => "hevc",
        "h264" | "avc" => "h264",
        "av01" | "av1" => "av1",
        "vp09" | "vp9" => "vp9",
        "vp08" | "vp8" => "vp8",
        "mpeg4" => "mpeg4",
        "eac3" | "e-ac-3" => "eac3",
        "ac3" | "ac-3" => "ac3",
        "dca" | "dts" => "dts",
        "truehd" => "truehd",
        "mp4a" | "aac" => "aac",
        "mp3" | "mp3float" => "mp3",
        "flac" => "flac",
        "opus" => "opus",
        "vorbis" => "vorbis",
        "subrip" | "srt" => "subrip",
        "ass" | "ssa" => "ass",
        "hdmv_pgs_subtitle" | "pgs" => "pgs",
        "mov_text" => "mov_text",
        // Unknown codec hand back the owned, already-lowercased string rather
        // than re-allocating a copy of it.
        _ => return raw,
    }
    .to_string()
}

// ----- Raw ffprobe JSON shapes -------------------------------------------------

#[derive(Debug, Deserialize)]
pub(super) struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfStream>,
    #[serde(default)]
    format: Option<FfFormat>,
    #[serde(default)]
    chapters: Vec<FfChapter>,
}

/// ffprobe `-show_chapters` entry: `start_time`/`end_time` are seconds as strings.
#[derive(Debug, Deserialize)]
struct FfChapter {
    start_time: Option<String>,
    end_time: Option<String>,
    #[serde(default)]
    tags: Option<FfChapterTags>,
}

#[derive(Debug, Deserialize)]
struct FfChapterTags {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    channels: Option<u32>,
    pix_fmt: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    bits_per_raw_sample: Option<String>,
    #[serde(default)]
    tags: Option<FfTags>,
    #[serde(default)]
    disposition: Option<FfDisposition>,
}

#[derive(Debug, Deserialize)]
struct FfTags {
    language: Option<String>,
    title: Option<String>,
}

/// ffprobe stream `disposition` flags we only read `default`.
#[derive(Debug, Deserialize)]
struct FfDisposition {
    default: Option<u8>,
}

impl FfStream {
    fn language(&self) -> Option<String> {
        self.tags
            .as_ref()
            .and_then(|t| t.language.clone())
            .filter(|l| !l.is_empty() && l != "und")
    }

    fn title(&self) -> Option<String> {
        self.tags
            .as_ref()
            .and_then(|t| t.title.clone())
            .filter(|t| !t.trim().is_empty())
    }
}
