//! Candle Whisper inference: PCM → log-mel → encoder/decoder greedy decode →
//! WebVTT. Adapted from candle-transformers' whisper example, simplified to a
//! greedy decoder with timestamp-delimited cues over 30 s windows. CPU-only.

use std::path::Path;

use anyhow::{anyhow, Result};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as w, audio, Config};
use tokenizers::Tokenizer;

const SAMPLE_RATE: f64 = 16000.0;
const N_FRAMES: usize = 3000; // 30 s of mel frames (hop 160 @ 16 kHz)
const HOP: f64 = 160.0;
const MAX_TOKENS: usize = 224;

/// Transcribe `pcm` (mono 16 kHz f32) with the model in `model_dir`. `None` on any
/// failure (logged); the caller falls back / reports an error.
pub fn transcribe(model_dir: &Path, pcm: &[f32], _lang: Option<&str>) -> Option<String> {
    match run(model_dir, pcm) {
        Ok(vtt) => Some(vtt),
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "whisper transcription failed");
            None
        }
    }
}

fn run(model_dir: &Path, pcm: &[f32]) -> Result<String> {
    let device = Device::Cpu;
    let cfg: Config = serde_json::from_reader(std::fs::File::open(model_dir.join("config.json"))?)?;
    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|e| anyhow!("{e}"))?;
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[model_dir.join("model.safetensors")], DType::F32, &device)? };
    let mut model = w::model::Whisper::load(&vb, cfg.clone())?;

    let filters = mel_filters(cfg.num_mel_bins);
    let mel = audio::pcm_to_mel(&cfg, pcm, &filters);
    let n_frames = mel.len() / cfg.num_mel_bins;
    let mel = Tensor::from_vec(mel, (1, cfg.num_mel_bins, n_frames), &device)?;

    let tok = |s: &str| tokenizer.token_to_id(s).ok_or_else(|| anyhow!("missing token {s}"));
    let sot = tok("<|startoftranscript|>")?;
    let eot = tok("<|endoftext|>")?;
    let transcribe = tok("<|transcribe|>")?;
    let no_ts = tok("<|notimestamps|>")?;
    let ts0 = tok("<|0.00|>")?;

    let mut out = String::from("WEBVTT\n\n");
    let mut seek = 0usize;
    while seek < n_frames {
        let len = (n_frames - seek).min(N_FRAMES);
        let mut window = mel.narrow(2, seek, len)?;
        if len < N_FRAMES {
            let pad = Tensor::zeros((1, cfg.num_mel_bins, N_FRAMES - len), DType::F32, &device)?;
            window = Tensor::cat(&[&window, &pad], 2)?;
        }
        let features = model.encoder.forward(&window, true)?;

        let mut tokens: Vec<u32> = vec![sot];
        for i in 0..MAX_TOKENS {
            let t = Tensor::new(tokens.as_slice(), &device)?.unsqueeze(0)?;
            let logits = model.decoder.forward(&t, &features, i == 0)?;
            let next = logits.i((0, logits.dim(1)? - 1))?.argmax(0)?.to_scalar::<u32>()?;
            if next == no_ts {
                continue;
            }
            tokens.push(next);
            if next == eot {
                break;
            }
        }

        let offset = seek as f64 * HOP / SAMPLE_RATE;
        emit_segments(&mut out, &tokens, &tokenizer, ts0, &[eot, transcribe, sot], offset);
        seek += N_FRAMES;
    }
    Ok(out)
}

/// Append cues for one window's tokens: text between two timestamp tokens is a
/// cue; a timestamp token maps to `(id - ts0) * 0.02` seconds + window `offset`.
fn emit_segments(out: &mut String, tokens: &[u32], tokenizer: &Tokenizer, ts0: u32, specials: &[u32], offset: f64) {
    let mut start: Option<f64> = None;
    let mut text_ids: Vec<u32> = Vec::new();
    for &t in tokens {
        if t >= ts0 {
            let time = offset + (t - ts0) as f64 * 0.02;
            if let Some(s) = start.take() {
                let text = tokenizer.decode(&text_ids, true).unwrap_or_default();
                let text = text.trim();
                if !text.is_empty() {
                    out.push_str(&format!("{} --> {}\n{}\n\n", fmt_ts(s), fmt_ts(time), text));
                }
            }
            text_ids.clear();
            start = Some(time);
        } else if !specials.contains(&t) {
            text_ids.push(t);
        }
    }
}

fn fmt_ts(s: f64) -> String {
    let s = s.max(0.0);
    let h = (s / 3600.0) as u64;
    let m = ((s % 3600.0) / 60.0) as u64;
    let sec = s % 60.0;
    format!("{h:02}:{m:02}:{sec:06.3}")
}

/// Slaney (librosa-compatible) mel filterbank for Whisper: sr=16000, n_fft=400.
/// Flattened `n_mels * (n_fft/2 + 1)`, matching `audio::pcm_to_mel`'s expectation.
fn mel_filters(n_mels: usize) -> Vec<f32> {
    const SR: f64 = 16000.0;
    const N_FFT: usize = 400;
    let n_freqs = N_FFT / 2 + 1;
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4_f64).ln() / 27.0;
    let hz_to_mel = |hz: f64| if hz >= min_log_hz { min_log_mel + (hz / min_log_hz).ln() / logstep } else { hz / f_sp };
    let mel_to_hz = |mel: f64| if mel >= min_log_mel { min_log_hz * (logstep * (mel - min_log_mel)).exp() } else { mel * f_sp };
    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(SR / 2.0);
    let hz_pts: Vec<f64> = (0..n_mels + 2)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64))
        .collect();
    let fft_freqs: Vec<f64> = (0..n_freqs).map(|i| (SR / 2.0) * i as f64 / (n_freqs - 1) as f64).collect();
    let mut weights = vec![0f32; n_mels * n_freqs];
    for m in 0..n_mels {
        let (lo, ce, hi) = (hz_pts[m], hz_pts[m + 1], hz_pts[m + 2]);
        let enorm = 2.0 / (hi - lo);
        for (k, &f) in fft_freqs.iter().enumerate() {
            let val = ((f - lo) / (ce - lo)).min((hi - f) / (hi - ce)).max(0.0) * enorm;
            weights[m * n_freqs + k] = val as f32;
        }
    }
    weights
}
