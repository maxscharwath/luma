//! AI transcription via Whisper. Two backends behind a common shape:
//! - `transcribe_cloud`: an OpenAI-compatible `POST /audio/transcriptions`
//!   (`response_format=vtt`). Cloud APIs cap upload size (~25 MB) so this suits
//!   episodes / short content; full movies need the local backend (or chunking).
//! - `transcribe_local`: the offline whisper.cpp CLI - no size limit, slower.
//!
//! Both extract a compact mono 16 kHz audio track with ffmpeg first. These are
//! long-running + blocking; the caller runs them off the async runtime.

use std::path::Path;
use std::process::Command;

/// Extract a single mono 16 kHz audio file. `wav` = PCM s16le (what whisper.cpp
/// expects); otherwise a small MP3 (smaller upload for the cloud API).
fn extract_audio(input: &Path, out: &Path, track: u32, wav: bool) -> bool {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .arg("-vn")
        .arg("-map")
        .arg(format!("0:a:{track}"))
        .args(["-ac", "1", "-ar", "16000"]);
    if wav {
        cmd.args(["-c:a", "pcm_s16le"]);
    } else {
        cmd.args(["-c:a", "libmp3lame", "-q:a", "9"]);
    }
    cmd.arg(out).status().map(|s| s.success()).unwrap_or(false)
}

/// Cloud transcription → WebVTT, or `None` on failure. `base_url` defaults to
/// OpenAI; `model` to `whisper-1`.
pub fn transcribe_cloud(api_key: &str, base_url: &str, model: &str, input: &Path, track: u32, scratch: &Path) -> Option<String> {
    if api_key.trim().is_empty() {
        return None;
    }
    let audio = scratch.join("audio.mp3");
    if !extract_audio(input, &audio, track, false) {
        return None;
    }
    let base = if base_url.trim().is_empty() { "https://api.openai.com/v1" } else { base_url.trim_end_matches('/') };
    let model = if model.trim().is_empty() { "whisper-1" } else { model.trim() };
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "900", "-X", "POST"])
        .arg("-H")
        .arg(format!("Authorization: Bearer {api_key}"))
        .arg("-F")
        .arg(format!("model={model}"))
        .arg("-F")
        .arg("response_format=vtt")
        .arg("-F")
        .arg(format!("file=@{}", audio.display()))
        .arg(format!("{base}/audio/transcriptions"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // A VTT body has cue arrows; an error response is JSON (no `-->`).
    text.contains("-->").then(|| ensure_header(text))
}

/// Local whisper.cpp: `<binary> -m <model> -f audio.wav -ovtt -of <out>` → read
/// `<out>.vtt`. `binary` defaults to `whisper-cli`; `model` is a GGUF path.
pub fn transcribe_local(binary: &str, model: &str, input: &Path, track: u32, scratch: &Path) -> Option<String> {
    if model.trim().is_empty() {
        return None;
    }
    let audio = scratch.join("audio.wav");
    if !extract_audio(input, &audio, track, true) {
        return None;
    }
    let bin = if binary.trim().is_empty() { "whisper-cli" } else { binary.trim() };
    let out_base = scratch.join("out");
    let status = Command::new(bin)
        .args(["-m", model.trim()])
        .arg("-f")
        .arg(&audio)
        .arg("-ovtt")
        .arg("-of")
        .arg(&out_base)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    std::fs::read_to_string(out_base.with_extension("vtt")).ok().filter(|s| s.contains("-->")).map(ensure_header)
}

/// Some transcribers emit cues without the leading `WEBVTT` line; add it.
fn ensure_header(text: String) -> String {
    if text.trim_start().starts_with("WEBVTT") {
        text
    } else {
        format!("WEBVTT\n\n{text}")
    }
}
