//! On-device subtitle generation. Two server-side engines, both producing WebVTT
//! that is cached under `<data>/subs/downloaded/` and recorded in the DB so it
//! shows in the item's subtitle list next to embedded tracks:
//! - **transcribe**: in-process Whisper (candle, [`crate::ports::Whisper`]) turns
//!   the audio into timestamped text. Model size is picked by [`Quality`].
//! - **translate**: the app's default LLM ([`translate`]) rewrites an existing
//!   text track into another language.
//!
//! Both are long-running + blocking (ffmpeg + model) and report progress through a
//! [`progress::Handle`] so the player can show a live bar + ETA and cancel.

mod translate;

pub mod progress;

use std::path::Path;

use crate::db::{self, DownloadedSub, Pool};
use crate::services::settings::Settings;

pub use progress::{GenRegistry, Handle};

/// What to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenMode {
    /// Whisper speech-to-text from an audio track.
    Transcribe,
    /// LLM translation of an existing subtitle track.
    Translate,
}

impl GenMode {
    pub fn parse(s: &str) -> GenMode {
        match s {
            "translate" => GenMode::Translate,
            _ => GenMode::Transcribe,
        }
    }

    /// The `provider` tag stored on the resulting track (drives the client's "IA"
    /// badge and the AI-track classification).
    fn provider(self) -> &'static str {
        match self {
            GenMode::Transcribe => "whisper",
            GenMode::Translate => "translate",
        }
    }
}

/// Whisper model tier (the player's Rapide / Équilibré / Précis selector). Maps to
/// a multilingual candle model auto-downloaded on first use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Fast,
    Balanced,
    Accurate,
}

impl Quality {
    pub fn parse(s: &str) -> Quality {
        match s {
            "fast" => Quality::Fast,
            "accurate" => Quality::Accurate,
            _ => Quality::Balanced,
        }
    }

    /// HuggingFace repo id for this tier (multilingual variants so spoken-language
    /// forcing works for non-English content).
    fn model(self) -> &'static str {
        match self {
            Quality::Fast => "openai/whisper-tiny",
            Quality::Balanced => "openai/whisper-base",
            Quality::Accurate => "openai/whisper-small",
        }
    }
}

/// A generation request, resolved server-side (the client never uploads audio or
/// subtitle text; for `Translate` the endpoint reads/extracts `source_vtt`).
pub struct GenSpec {
    pub mode: GenMode,
    /// Target language label, e.g. "Français".
    pub target_lang: String,
    /// Transcribe only: the spoken language to force (name or code); `None` =
    /// auto-detect.
    pub spoken_lang: Option<String>,
    /// Transcribe only: model tier.
    pub quality: Quality,
    /// Transcribe only: audio-relative track index.
    pub audio_track: u32,
    /// Translate only: the source track's WebVTT text.
    pub source_vtt: Option<String>,
}

/// Run a generation to completion, caching + recording the WebVTT. Reports stage +
/// progress through `handle` and bails early when cancelled. `Ok(record)` on
/// success; `Err(reason)` carries *why* it failed (no LLM provider, an LLM/Whisper
/// error, an empty result, a write/DB error) so the caller can show it instead of a
/// blank "generation failed". Blocking - call off the async runtime.
// Threads the whole generation context (settings, IO, spec, ports); a struct would just move the noise.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    settings: &Settings,
    data_dir: &Path,
    pool: &Pool,
    item_id: &str,
    input: &Path,
    spec: &GenSpec,
    handle: &Handle,
    whisper: &dyn crate::ports::Whisper,
) -> std::result::Result<DownloadedSub, String> {
    let vtt = match spec.mode {
        GenMode::Transcribe => {
            let code = spec.spoken_lang.as_deref().and_then(lang_to_code);
            let cancel = handle.cancel_flag();
            whisper
                .transcribe(
                    data_dir,
                    spec.quality.model(),
                    input,
                    spec.audio_track,
                    code,
                    &|stage| handle.stage(stage),
                    &|done, total| handle.progress(done, total),
                    &cancel,
                )
                .ok_or_else(|| {
                    "transcription produced no text (wrong audio track, or the Whisper model failed to load; see server logs)".to_string()
                })?
        }
        GenMode::Translate => {
            handle.stage("translate");
            let src = spec
                .source_vtt
                .as_deref()
                .ok_or_else(|| "no source subtitle track to translate from".to_string())?;
            translate::translate_vtt(settings, src, &spec.target_lang, handle)?
        }
    };
    if vtt.len() < 16 {
        return Err("the generated subtitle came out empty".to_string());
    }

    let provider = spec.mode.provider();
    let id = stable_id(item_id, provider, &spec.target_lang);
    let dir = data_dir.join("subs").join("downloaded");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create the subtitle cache dir: {e}"))?;
    let path = dir.join(format!("{id}.vtt"));
    std::fs::write(&path, vtt.as_bytes()).map_err(|e| format!("could not write the subtitle file: {e}"))?;

    let sub = DownloadedSub {
        id,
        item_id: item_id.to_string(),
        // Store the ISO code, or `None` for an unrecognized language - never the raw
        // display label (e.g. "Français"), which is not a valid language code.
        language: lang_to_code(&spec.target_lang).map(str::to_string),
        label: spec.target_lang.clone(),
        provider: provider.to_string(),
        path: path.to_string_lossy().into_owned(),
    };
    db::insert_downloaded_sub(pool, &sub).map_err(|e| format!("could not record the subtitle in the database: {e}"))?;
    Ok(sub)
}

/// Deterministic id for a (item, provider, target) triple so re-generating the same
/// language replaces rather than duplicates.
fn stable_id(item_id: &str, provider: &str, target_lang: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    item_id.hash(&mut h);
    provider.hash(&mut h);
    target_lang.to_lowercase().hash(&mut h);
    format!("dl{:016x}", h.finish())
}

/// Normalize text to WebVTT (SRT timestamps `,`→`.` + header); a body already
/// starting with `WEBVTT` is passed through. Used for source tracks fed to translate.
pub fn to_vtt(raw: &str) -> String {
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

/// Map a spoken-language name or code to an ISO 639-1 code for Whisper. Accepts the
/// French + English names the UI offers, or an already-2-letter code. `None` when
/// unrecognized (Whisper then auto-detects).
fn lang_to_code(input: &str) -> Option<&'static str> {
    let s = input.trim().to_lowercase();
    let code = match s.as_str() {
        "fr" | "french" | "français" | "francais" => "fr",
        "en" | "english" | "anglais" => "en",
        "es" | "spanish" | "espagnol" | "español" | "espanol" => "es",
        "de" | "german" | "allemand" | "deutsch" => "de",
        "it" | "italian" | "italien" | "italiano" => "it",
        "pt" | "portuguese" | "portugais" | "português" | "portugues" => "pt",
        "nl" | "dutch" | "néerlandais" | "neerlandais" => "nl",
        "ja" | "japanese" | "japonais" => "ja",
        "ko" | "korean" | "coréen" | "coreen" => "ko",
        "zh" | "chinese" | "chinois" => "zh",
        "ru" | "russian" | "russe" => "ru",
        "ar" | "arabic" | "arabe" => "ar",
        "hi" | "hindi" => "hi",
        "pl" | "polish" | "polonais" => "pl",
        "tr" | "turkish" | "turc" => "tr",
        "sv" | "swedish" | "suédois" | "suedois" => "sv",
        _ => return None,
    };
    Some(code)
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
    fn stable_id_is_deterministic_and_case_insensitive() {
        assert_eq!(stable_id("a", "whisper", "French"), stable_id("a", "whisper", "french"));
        assert_ne!(stable_id("a", "whisper", "French"), stable_id("a", "whisper", "English"));
    }

    #[test]
    fn lang_codes() {
        assert_eq!(lang_to_code("Français"), Some("fr"));
        assert_eq!(lang_to_code("english"), Some("en"));
        assert_eq!(lang_to_code("es"), Some("es"));
        assert_eq!(lang_to_code("Klingon"), None);
    }

    #[test]
    fn lang_codes_cover_many_names_and_codes() {
        assert_eq!(lang_to_code("  DEUTSCH "), Some("de"));
        assert_eq!(lang_to_code("italiano"), Some("it"));
        assert_eq!(lang_to_code("Japonais"), Some("ja"));
        assert_eq!(lang_to_code("zh"), Some("zh"));
        assert_eq!(lang_to_code("russe"), Some("ru"));
        assert_eq!(lang_to_code("português"), Some("pt"));
        assert_eq!(lang_to_code(""), None);
    }

    #[test]
    fn gen_mode_parse_and_provider() {
        assert_eq!(GenMode::parse("translate"), GenMode::Translate);
        assert_eq!(GenMode::parse("transcribe"), GenMode::Transcribe);
        assert_eq!(GenMode::parse("anything-else"), GenMode::Transcribe);
        assert_eq!(GenMode::Transcribe.provider(), "whisper");
        assert_eq!(GenMode::Translate.provider(), "translate");
    }

    #[test]
    fn quality_parse_and_model() {
        assert_eq!(Quality::parse("fast"), Quality::Fast);
        assert_eq!(Quality::parse("accurate"), Quality::Accurate);
        assert_eq!(Quality::parse("balanced"), Quality::Balanced);
        assert_eq!(Quality::parse("bogus"), Quality::Balanced);
        assert_eq!(Quality::Fast.model(), "openai/whisper-tiny");
        assert_eq!(Quality::Balanced.model(), "openai/whisper-base");
        assert_eq!(Quality::Accurate.model(), "openai/whisper-small");
    }

    #[test]
    fn to_vtt_strips_bom_and_converts_srt_commas() {
        let srt = "\u{feff}1\n00:00:01,000 --> 00:00:04,000\nLine one\nLine two\n";
        let vtt = to_vtt(srt);
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:01.000 --> 00:00:04.000"));
        assert!(vtt.contains("Line one"));
        assert!(vtt.contains("Line two"));
    }

    #[test]
    fn to_vtt_passthrough_after_bom() {
        let with_bom = "\u{feff}WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi\n";
        // BOM stripped, already-WEBVTT passes through unchanged.
        assert_eq!(to_vtt(with_bom), "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi\n");
    }

    #[test]
    fn stable_id_distinguishes_item_and_provider() {
        assert_ne!(stable_id("a", "whisper", "French"), stable_id("b", "whisper", "French"));
        assert_ne!(stable_id("a", "whisper", "French"), stable_id("a", "translate", "French"));
        assert!(stable_id("a", "whisper", "French").starts_with("dl"));
    }
}
