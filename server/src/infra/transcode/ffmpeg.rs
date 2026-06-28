//! Building the `ffmpeg` HLS remux commands: the arg/flag construction for a
//! single-track variant and for the multi-rendition master, plus spawning them.

use std::path::Path;
use std::process::Stdio;

use tokio::process::{Child, Command};

/// HLS target segment duration handed to ffmpeg.
const SEGMENT_SECONDS: &str = "6";

/// One audio rendition in a master remux: which source audio track to map and
/// how to label it. v1 stream-copies every track (so the runtime must natively
/// decode them — gated client-side by `canSeamlessAudioSwitch`).
pub struct MasterTrack {
    /// Audio-relative source index (`-map 0:a:<index>`).
    pub index: u32,
    /// BCP-47-ish language tag for the rendition (sanitised before use).
    pub language: Option<String>,
    /// Marks the rendition the player selects by default (exactly one should be).
    pub default: bool,
}

/// Build the ffmpeg **master**-playlist command: copy the video once and copy
/// every listed audio track as an alternate HLS rendition (audio group `aud`), so
/// the player switches language in place. Emits master.m3u8 + per-variant
/// playlists/segments (fMP4): `stream_%v.m3u8`, `init_%v.mp4`, `seg_%v_*.m4s`.
pub(super) fn spawn_ffmpeg_master(input: &Path, dir: &Path, tracks: &[MasterTrack], aac: bool, start_secs: f64) -> std::io::Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin"]);
    // Input seeking: start the remux at `start_secs` so the requested position is
    // available immediately (no waiting for a from-zero remux to reach it).
    let seeking = start_secs > 0.5;
    if seeking {
        cmd.arg("-ss").arg(format!("{start_secs:.3}"));
    }
    cmd.arg("-i").arg(input);
    // `-copyts` keeps the ORIGINAL timestamps so every rendition (the copied video
    // + each audio rendition, which are separate outputs) stays on one shared
    // timeline and the player aligns them by PTS — without it each output is zeroed
    // independently and the keyframe-snapped video drifts out of sync with the
    // audio. The player still normalises the visible start to 0, so the client's
    // baseSec offset (added back for the bar/subtitles/progress) is unaffected.
    if seeking {
        cmd.arg("-copyts");
    }
    cmd.args(["-map", "0:v:0"]);
    for t in tracks {
        cmd.arg("-map").arg(format!("0:a:{}", t.index));
    }
    // Video is always stream-copied. Audio is copied (surround preserved, for
    // runtimes that decode it) or transcoded to stereo AAC (so browsers that
    // can't decode AC3/EAC3/DTS via MSE can still play — and switch — every track).
    cmd.args(["-c:v", "copy"]);
    if aac {
        cmd.args(["-c:a", "aac", "-ac", "2", "-b:a", "192k"]);
    } else {
        cmd.args(["-c:a", "copy"]);
    }

    // var_stream_map: one video variant + one variant per audio rendition, all in
    // the `aud` group so they're alternates of the same program.
    let mut map = String::from("v:0,agroup:aud");
    for (i, t) in tracks.iter().enumerate() {
        map.push_str(&format!(" a:{i},agroup:aud"));
        let lang: String = t
            .language
            .as_deref()
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(8)
            .collect();
        if !lang.is_empty() {
            map.push_str(&format!(",language:{lang}"));
        }
        if t.default {
            map.push_str(",default:yes");
        }
    }

    cmd.args(["-f", "hls", "-hls_time", SEGMENT_SECONDS])
        .args(["-hls_playlist_type", "event"])
        .args(["-hls_segment_type", "fmp4"])
        .args(["-hls_fmp4_init_filename", "init_%v.mp4"])
        .arg("-hls_segment_filename")
        .arg(dir.join("seg_%v_%05d.m4s"))
        .args(["-hls_flags", "independent_segments+temp_file"])
        .args(["-master_pl_name", "master.m3u8"])
        .arg("-var_stream_map")
        .arg(map)
        .arg(dir.join("stream_%v.m3u8"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Build the ffmpeg HLS command: copy the video stream verbatim, select audio
/// track `audio_idx`, and either stream-copy it (`copy`, preserving surround
/// with no re-encode) or transcode it to stereo AAC for runtimes that can't
/// decode the source codec. Emits fragmented-MP4 segments.
pub(super) fn spawn_ffmpeg(input: &Path, dir: &Path, audio_idx: u32, copy: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(input)
        // First video + the chosen audio track; ignore extra streams (subs/data)
        // the HLS muxer can't carry in fMP4.
        .args(["-map", "0:v:0"])
        .arg("-map")
        .arg(format!("0:a:{audio_idx}"))
        .args(["-c:v", "copy"]);
    if copy {
        cmd.args(["-c:a", "copy"]);
    } else {
        cmd.args(["-c:a", "aac", "-ac", "2", "-b:a", "192k"]);
    }
    cmd.args(["-f", "hls", "-hls_time", SEGMENT_SECONDS])
        .args(["-hls_playlist_type", "event"])
        .args(["-hls_segment_type", "fmp4"])
        .args(["-hls_fmp4_init_filename", "init.mp4"])
        .arg("-hls_segment_filename")
        .arg(dir.join("seg_%05d.m4s"))
        // `temp_file` → write to `.tmp` then atomically rename, so we never serve
        // a half-written segment/playlist.
        .args(["-hls_flags", "independent_segments+temp_file"])
        .arg(dir.join("index.m3u8"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd.spawn()
}
