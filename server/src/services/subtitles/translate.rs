//! AI subtitle translation: translate an existing WebVTT track into another
//! language using the app's DEFAULT LLM provider (the one configured on the admin
//! IA page). Timestamps are preserved verbatim; only the cue text is translated,
//! in batches to keep each prompt small.

use crate::infra::llm::{build_http, LlmClient};
use crate::services::settings::{self, Settings};

/// One parsed cue: its timing line (`00:00:01.000 --> 00:00:04.000` [+ settings])
/// and the joined text lines.
struct Cue {
    timing: String,
    text: String,
}

fn parse_cues(vtt: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    let mut timing: Option<String> = None;
    let mut text: Vec<String> = Vec::new();
    let flush = |timing: &mut Option<String>, text: &mut Vec<String>, cues: &mut Vec<Cue>| {
        if let Some(t) = timing.take() {
            cues.push(Cue { timing: t, text: text.join("\n") });
        }
        text.clear();
    };
    for line in vtt.lines() {
        if line.contains("-->") {
            flush(&mut timing, &mut text, &mut cues);
            timing = Some(line.trim().to_string());
        } else if timing.is_some() {
            if line.trim().is_empty() {
                flush(&mut timing, &mut text, &mut cues);
            } else {
                text.push(line.to_string());
            }
        }
    }
    flush(&mut timing, &mut text, &mut cues);
    cues
}

const BATCH: usize = 40;

/// Translate `vtt` into `target_lang` (a language name like "French"). Returns the
/// translated WebVTT, or `None` if no LLM is configured or every batch failed.
/// Blocking (the LLM client shells out) - call off-thread.
pub fn translate_vtt(settings: &Settings, vtt: &str, target_lang: &str) -> Option<String> {
    let p = settings::default_provider(settings)?;
    let llm = build_http(&p.provider, &p.base_url, &p.model, &p.api_key, 0.2, p.reasoning)?;
    let cues = parse_cues(vtt);
    if cues.is_empty() {
        return None;
    }
    let mut out = String::from("WEBVTT\n\n");
    let mut any = false;
    for batch in cues.chunks(BATCH) {
        let translated = translate_batch(llm.as_ref(), batch, target_lang);
        for (i, cue) in batch.iter().enumerate() {
            let line = translated.as_ref().and_then(|v| v.get(i)).map(String::as_str).unwrap_or(&cue.text);
            out.push_str(&cue.timing);
            out.push('\n');
            out.push_str(line.trim());
            out.push_str("\n\n");
        }
        any |= translated.is_some();
    }
    any.then_some(out)
}

/// Ask the LLM to translate a batch of cue texts, one per numbered line, and parse
/// the numbered reply back into the same order. Falls back to `None` on any shape
/// mismatch so the caller keeps the originals for that batch.
fn translate_batch(llm: &dyn LlmClient, batch: &[Cue], target_lang: &str) -> Option<Vec<String>> {
    let numbered: String =
        batch.iter().enumerate().map(|(i, c)| format!("{}. {}\n", i + 1, c.text.replace('\n', " "))).collect();
    let system = format!(
        "You are a professional subtitle translator. Translate each numbered subtitle line into {target_lang}. \
         Output EXACTLY the same number of lines, each prefixed with its number and a period, and NOTHING else. \
         Preserve meaning and tone; keep proper nouns. Do not merge or split lines."
    );
    let reply = llm.complete(&system, &numbered, (batch.len() as u32) * 80 + 200).ok()?;
    let mut out = vec![String::new(); batch.len()];
    let mut filled = 0;
    for line in reply.lines() {
        let line = line.trim();
        let Some((num, rest)) = line.split_once('.') else { continue };
        if let Ok(n) = num.trim().parse::<usize>() {
            if n >= 1 && n <= batch.len() {
                out[n - 1] = rest.trim().to_string();
                filled += 1;
            }
        }
    }
    // Require most lines to have parsed, else treat the batch as failed.
    (filled * 2 >= batch.len()).then_some(out)
}
