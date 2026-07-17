//! AI subtitle translation: translate an existing WebVTT track into another
//! language using the app's configured LLM providers in failover order (default
//! first, then the rest, e.g. cloud OpenRouter then a local Ollama). Timestamps
//! are preserved verbatim; only the cue text is translated, in batches to keep
//! each prompt small. A provider that is out of credits / rate-limited / down is
//! skipped for the next provider, sticking with whichever one works.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tracing::{info, warn};

use crate::infra::llm::{build_http, LlmClient};
use crate::services::settings::{self, Settings};
use crate::services::subtitles::progress::Handle;

/// Sampling temperature for translation: low, for deterministic, format-faithful
/// output regardless of each provider's configured creativity.
const TRANSLATE_TEMP: f32 = 0.2;

/// One usable provider in the failover chain: its client plus its own token
/// budget (a constrained cloud account and a roomy local model differ).
struct Backend {
    label: String,
    client: Arc<dyn LlmClient>,
    token_cap: u32,
}

/// Build the ordered, usable provider chain (default first, then the rest), each
/// at the low translation temperature. Providers whose config can't form a client
/// are skipped; the result is empty only when nothing is configured.
fn build_backends(settings: &Settings) -> Vec<Backend> {
    settings::ordered_providers(settings)
        .into_iter()
        .filter_map(|p| {
            let client =
                build_http(&p.provider, p.base_url.trim(), p.model.trim(), p.api_key.trim(), TRANSLATE_TEMP, p.reasoning)?;
            let name = if p.name.trim().is_empty() { p.provider.clone() } else { p.name.clone() };
            Some(Backend {
                label: format!("{name} ({})", p.model),
                client,
                // Respect each provider's configured output cap (translate used to
                // ignore it and always ask for BATCH*80+200 tokens, which a
                // low-credit account rejects outright). Clamp like the other jobs.
                token_cap: p.max_tokens.clamp(64, 8192) as u32,
            })
        })
        .collect()
}

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

/// Cues per LLM request. Smaller batches keep each request cheap (so it fits a
/// modest token budget / a low-credit account) and isolate a failing batch.
const BATCH: usize = 24;

/// How many batches to translate concurrently. The batches are independent, so a
/// cloud provider (OpenRouter/OpenAI/Anthropic) parallelizes the round-trips for a
/// big wall-clock win; a single-slot local Ollama just queues them (no harm). Kept
/// modest so a rate-limited account isn't hammered.
const PARALLEL: usize = 4;

/// Translate `vtt` into `target_lang` (a language name like "French"). Reports
/// per-batch progress through `handle` and bails when cancelled. `Ok(webvtt)` on
/// success (including a partial translation where some batches kept their
/// originals); `Err(reason)` carries *why* it could not run at all (no provider,
/// every batch failed, cancelled, …) so the caller can surface it instead of a
/// blank "generation failed". Blocking (the LLM client shells out) - call off-thread.
pub fn translate_vtt(
    settings: &Settings,
    vtt: &str,
    target_lang: &str,
    handle: &Handle,
) -> std::result::Result<String, String> {
    let backends = build_backends(settings);
    if backends.is_empty() {
        return Err("no LLM provider configured (set one on the admin IA page)".to_string());
    }
    let cues = parse_cues(vtt);
    if cues.is_empty() {
        return Err("the source subtitle had no cues to translate".to_string());
    }
    let total = cues.len();
    let chunks: Vec<&[Cue]> = cues.chunks(BATCH).collect();
    let batches = chunks.len();
    let chain = backends.iter().map(|b| b.label.as_str()).collect::<Vec<_>>().join(" -> ");
    let workers = PARALLEL.min(batches).max(1);
    info!(target = %target_lang, cues = total, batches, workers, %chain, "subtitle translate: starting");
    handle.progress(0, total);

    // Shared work state pulled by `workers` scoped threads: a batch cursor, the
    // sticky provider hint, a running done-count for progress, per-batch result
    // slots, the first hard error, and how many batches produced any translation.
    let next = AtomicUsize::new(0);
    let active = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let translated = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<Vec<Option<String>>>>> = (0..batches).map(|_| Mutex::new(None)).collect();
    let first_error: Mutex<Option<String>> = Mutex::new(None);

    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                if handle.cancelled() {
                    break;
                }
                let bi = next.fetch_add(1, Ordering::Relaxed);
                if bi >= batches {
                    break;
                }
                let batch = chunks[bi];
                match translate_one(&backends, &active, batch, target_lang) {
                    Ok(v) => {
                        *results[bi].lock().unwrap() = Some(v);
                        translated.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        warn!(batch = bi + 1, total = batches, "subtitle translate: batch failed on every provider: {e}");
                        let mut fe = first_error.lock().unwrap();
                        if fe.is_none() {
                            *fe = Some(e);
                        }
                    }
                }
                let d = done.fetch_add(batch.len(), Ordering::Relaxed) + batch.len();
                handle.progress(d, total);
            });
        }
    });

    if handle.cancelled() {
        return Err("cancelled".to_string());
    }
    let ok_batches = translated.load(Ordering::Relaxed);
    if ok_batches == 0 {
        // Every batch failed on every provider: a real failure, and `first_error`
        // holds the LLM's actual complaint (auth, model, credits, parse, …).
        return Err(first_error.into_inner().unwrap().unwrap_or_else(|| "translation failed for every batch".to_string()));
    }
    if ok_batches < batches {
        warn!(ok = ok_batches, total = batches, "subtitle translate: finished with some batches left untranslated");
    } else {
        info!(batches, "subtitle translate: done");
    }

    // Reassemble in cue order; an untranslated batch or a per-line gap falls back to
    // the ORIGINAL text (never a blank line).
    let mut out = String::from("WEBVTT\n\n");
    for (bi, batch) in chunks.iter().enumerate() {
        let res = results[bi].lock().unwrap();
        for (i, cue) in batch.iter().enumerate() {
            let line = res
                .as_ref()
                .and_then(|v| v.get(i))
                .and_then(|o| o.as_deref())
                .filter(|s| !s.is_empty())
                .unwrap_or(&cue.text);
            out.push_str(&cue.timing);
            out.push('\n');
            out.push_str(line.trim());
            out.push_str("\n\n");
        }
    }
    Ok(out)
}

/// Translate one batch, trying the currently-active backend first and falling
/// through to the next on failure. Sticks with whichever backend succeeds (sets
/// `active`), so a dead primary is not re-hit on every batch. `Err` only when
/// *every* remaining backend fails this batch (carrying the first reason).
fn translate_one(
    backends: &[Backend],
    active: &AtomicUsize,
    batch: &[Cue],
    target_lang: &str,
) -> std::result::Result<Vec<Option<String>>, String> {
    let start = active.load(Ordering::Relaxed).min(backends.len().saturating_sub(1));
    let mut first_err: Option<String> = None;
    for (i, b) in backends.iter().enumerate().skip(start) {
        match translate_batch(b.client.as_ref(), batch, target_lang, b.token_cap) {
            Ok(v) => {
                if i != start {
                    info!(backend = %b.label, "subtitle translate: switched provider (previous one failed)");
                    active.store(i, Ordering::Relaxed);
                }
                return Ok(v);
            }
            Err(e) => {
                warn!(backend = %b.label, "subtitle translate: provider failed: {e}");
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    Err(first_err.unwrap_or_else(|| "no usable LLM provider".to_string()))
}

/// Ask the LLM to translate a batch of cue texts, one per numbered line, and parse
/// the numbered reply back into the same order. `Err(reason)` on an LLM error or a
/// reply that doesn't match the numbered shape, so the caller keeps the originals
/// for that batch *and* learns why.
fn translate_batch(
    llm: &dyn LlmClient,
    batch: &[Cue],
    target_lang: &str,
    token_cap: u32,
) -> std::result::Result<Vec<Option<String>>, String> {
    let numbered: String =
        batch.iter().enumerate().map(|(i, c)| format!("{}. {}\n", i + 1, c.text.replace('\n', " "))).collect();
    let system = format!(
        "You are a professional subtitle translator. Translate each numbered subtitle line into {target_lang}. \
         Output EXACTLY the same number of lines, each prefixed with its number and a period, and NOTHING else. \
         Preserve meaning and tone; keep proper nouns. Do not merge or split lines."
    );
    // Enough headroom for the numbered translation, but never above the provider's
    // configured cap (which is what a constrained account can actually afford).
    let max_tokens = ((batch.len() as u32) * 80 + 200).min(token_cap);
    let reply = llm
        .complete(&system, &numbered, max_tokens)
        .map_err(|e| format!("LLM request failed: {e:#}"))?;
    // Per-line result: `Some` for a parsed line, `None` for a gap (the caller keeps
    // that cue's ORIGINAL text rather than blanking it).
    let mut out: Vec<Option<String>> = vec![None; batch.len()];
    let mut filled = 0;
    for line in reply.lines() {
        let line = line.trim();
        let Some((num, rest)) = line.split_once('.') else { continue };
        if let Ok(n) = num.trim().parse::<usize>() {
            let rest = rest.trim();
            if n >= 1 && n <= batch.len() && !rest.is_empty() {
                out[n - 1] = Some(rest.to_string());
                filled += 1;
            }
        }
    }
    // Require most lines to have parsed, else treat the batch as failed (so failover
    // tries the next provider) and show a snippet of what the model actually returned.
    if filled * 2 >= batch.len() {
        Ok(out)
    } else {
        Err(format!(
            "model reply did not match the numbered format ({filled}/{} lines parsed); reply began: {}",
            batch.len(),
            snippet(&reply),
        ))
    }
}

/// A short, single-line snippet of an LLM reply for an error message.
fn snippet(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > 160 {
        format!("{}…", one_line.chars().take(160).collect::<String>())
    } else {
        one_line
    }
}
