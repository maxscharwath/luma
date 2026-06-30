//! Online subtitle search + download, provider-agnostic. A search returns
//! [`RemoteSub`] hits; a download fetches the chosen file, converts it to WebVTT,
//! caches it under `<data>/subs/downloaded/`, and records it in the DB so it shows
//! in the item's subtitle list. Only OpenSubtitles is wired today; the shape lets
//! more providers slot in later.

mod opensubtitles;
mod translate;
mod whisper;

use std::hash::{Hash, Hasher};
use std::path::Path;

use serde::Serialize;

use crate::db::{self, DownloadedSub, Pool};
use crate::services::settings::{Settings, SubtitleProvider};

/// A provider search hit (before download). `id` is provider-specific (the file
/// id to download). `downloads` is the provider's popularity count, for sorting.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteSub {
    pub id: String,
    pub provider: String,
    pub language: String,
    pub label: String,
    pub downloads: u32,
}

/// Provider credentials, read from settings by the caller so this layer stays
/// independent of the settings store.
#[derive(Debug, Clone, Default)]
pub struct Creds {
    pub os_api_key: String,
    pub os_username: String,
    pub os_password: String,
}

/// Search configured providers for `title` (optional `year`), restricted to
/// `langs` (e.g. `["fr","en"]`). Blocking (shells out via curl) - call off-thread.
pub fn search(creds: &Creds, title: &str, year: Option<i64>, langs: &[String]) -> Vec<RemoteSub> {
    opensubtitles::search(&creds.os_api_key, title, year, langs)
}

/// Download `remote_id` from `provider`, convert to WebVTT, cache it under
/// `<data_dir>/subs/downloaded/`, and record it for `item_id`. Returns the new
/// record (or `None` on any failure). Blocking - call off-thread.
pub fn download(
    creds: &Creds,
    data_dir: &Path,
    pool: &Pool,
    item_id: &str,
    provider: &str,
    remote_id: &str,
    language: Option<&str>,
    label: &str,
) -> Option<DownloadedSub> {
    let raw = match provider {
        "opensubtitles" => opensubtitles::download(&creds.os_api_key, &creds.os_username, &creds.os_password, remote_id)?,
        _ => return None,
    };
    let vtt = to_vtt(&raw);
    if vtt.len() < 16 {
        return None; // empty / not a subtitle
    }

    let id = stable_id(item_id, provider, remote_id);
    let dir = data_dir.join("subs").join("downloaded");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{id}.vtt"));
    std::fs::write(&path, vtt.as_bytes()).ok()?;

    let sub = DownloadedSub {
        id,
        item_id: item_id.to_string(),
        language: language.map(str::to_string),
        label: label.to_string(),
        provider: provider.to_string(),
        path: path.to_string_lossy().into_owned(),
    };
    db::insert_downloaded_sub(pool, &sub).ok()?;
    Some(sub)
}

/// Deterministic id for a (item, provider, remote) triple so re-downloading the
/// same track replaces rather than duplicates.
fn stable_id(item_id: &str, provider: &str, remote_id: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    item_id.hash(&mut h);
    provider.hash(&mut h);
    remote_id.hash(&mut h);
    format!("dl{:016x}", h.finish())
}

/// Normalize a downloaded subtitle to WebVTT. Most downloads are SRT (the only
/// difference that matters for the parser is `,` → `.` in timestamps and the
/// `WEBVTT` header); a file that already starts with `WEBVTT` is passed through.
fn to_vtt(raw: &str) -> String {
    let text = raw.trim_start_matches('\u{feff}');
    if text.trim_start().starts_with("WEBVTT") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 16);
    out.push_str("WEBVTT\n\n");
    for line in text.lines() {
        if line.contains("-->") {
            out.push_str(&line.replace(',', "."));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Generate a subtitle with an AI provider and cache + record it like a download:
/// transcribe the audio (`whisper` / `whisperLocal`) or translate `source_vtt`
/// into `target_lang` (`translate`). `audio_track` is the audio-relative index to
/// transcribe. Blocking (ffmpeg / network / CPU) - call off-thread.
pub fn generate(
    settings: &Settings,
    provider: &SubtitleProvider,
    data_dir: &Path,
    pool: &Pool,
    item_id: &str,
    input: &Path,
    audio_track: u32,
    target_lang: &str,
    source_vtt: Option<&str>,
) -> Option<DownloadedSub> {
    let scratch = data_dir.join("subs").join("tmp").join(format!("{item_id}-{}", provider.id));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).ok()?;
    let vtt = match provider.kind.as_str() {
        "whisper" => {
            whisper::transcribe_cloud(&provider.api_key, &provider.base_url, &provider.model, input, audio_track, &scratch)
        }
        "whisperLocal" => {
            // Prefer the in-process candle engine (model = a HF repo id / candle
            // dir); fall back to an external whisper.cpp binary (base_url = binary,
            // model = GGUF path) when the feature is off or the model isn't candle.
            crate::infra::whisper::transcribe(data_dir, &provider.model, input, audio_track, Some(target_lang))
                .or_else(|| whisper::transcribe_local(&provider.base_url, &provider.model, input, audio_track, &scratch))
        }
        "translate" => source_vtt.and_then(|s| translate::translate_vtt(settings, s, target_lang)),
        _ => None,
    };
    let _ = std::fs::remove_dir_all(&scratch);
    let vtt = vtt?;
    if vtt.len() < 16 {
        return None;
    }

    let id = stable_id(item_id, &provider.kind, target_lang);
    let dir = data_dir.join("subs").join("downloaded");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{id}.vtt"));
    std::fs::write(&path, vtt.as_bytes()).ok()?;

    let name = if provider.name.trim().is_empty() { kind_label(&provider.kind).to_string() } else { provider.name.clone() };
    let sub = DownloadedSub {
        id,
        item_id: item_id.to_string(),
        language: Some(target_lang.to_string()),
        label: format!("{name} · {target_lang}"),
        provider: provider.kind.clone(),
        path: path.to_string_lossy().into_owned(),
    };
    db::insert_downloaded_sub(pool, &sub).ok()?;
    Some(sub)
}

/// Whether a provider kind generates (vs searches a database).
pub fn is_ai_kind(kind: &str) -> bool {
    matches!(kind, "whisper" | "whisperLocal" | "translate")
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "whisper" | "whisperLocal" => "AI transcription",
        "translate" => "AI translation",
        _ => "Subtitle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_becomes_vtt() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\nHello\n";
        let vtt = to_vtt(srt);
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:01.000 --> 00:00:04.000"));
        assert!(vtt.contains("Hello"));
    }

    #[test]
    fn vtt_passthrough() {
        let vtt_in = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi\n";
        assert_eq!(to_vtt(vtt_in).trim(), vtt_in.trim());
    }

    #[test]
    fn stable_id_is_deterministic() {
        assert_eq!(stable_id("a", "opensubtitles", "1"), stable_id("a", "opensubtitles", "1"));
        assert_ne!(stable_id("a", "opensubtitles", "1"), stable_id("a", "opensubtitles", "2"));
    }
}
