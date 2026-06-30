//! In-process Whisper transcription (no external whisper.cpp binary). The heavy
//! candle inference lives in [`engine`], behind the `whisper-local` feature; the
//! default build compiles a stub so the `whisperLocal` provider transparently
//! falls back to the external binary.
//!
//! The model (`config.json` + `model.safetensors` + `tokenizer.json`) is either a
//! local directory or a HuggingFace repo id (e.g. `openai/whisper-base`), which is
//! downloaded once into `<data>/whisper/<repo>/`.

#[cfg(feature = "whisper-local")]
mod engine;

use std::path::Path;

/// Transcribe audio track `track` of `input` to WebVTT using the model at
/// `model_spec` (a local dir or a HF repo id). `lang` optionally forces the
/// spoken language (else auto-detected). `None` on failure / feature off.
#[allow(unused_variables)]
pub fn transcribe(data_dir: &Path, model_spec: &str, input: &Path, track: u32, lang: Option<&str>) -> Option<String> {
    #[cfg(feature = "whisper-local")]
    {
        let dir = resolve_model(data_dir, model_spec)?;
        let pcm = extract_pcm(input, track)?;
        engine::transcribe(&dir, &pcm, lang)
    }
    #[cfg(not(feature = "whisper-local"))]
    {
        None
    }
}

/// Extract the audio track as mono 16 kHz f32 PCM (Whisper's input format).
#[cfg(feature = "whisper-local")]
fn extract_pcm(input: &Path, track: u32) -> Option<Vec<f32>> {
    use std::process::Command;
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(input)
        .arg("-vn")
        .arg("-map")
        .arg(format!("0:a:{track}"))
        .args(["-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    Some(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// Resolve the model to a local directory, downloading from HuggingFace when
/// `spec` is a `owner/repo` id rather than an existing path.
#[cfg(feature = "whisper-local")]
fn resolve_model(data_dir: &Path, spec: &str) -> Option<std::path::PathBuf> {
    let spec = spec.trim();
    let local = Path::new(spec);
    if local.join("config.json").exists() && local.join("tokenizer.json").exists() {
        return Some(local.to_path_buf());
    }
    // Treat as a HF repo id; cache under <data>/whisper/<repo>.
    if !spec.contains('/') {
        return None;
    }
    let dir = data_dir.join("whisper").join(spec.replace('/', "_"));
    std::fs::create_dir_all(&dir).ok()?;
    for file in ["config.json", "tokenizer.json", "model.safetensors"] {
        let dest = dir.join(file);
        if dest.exists() && std::fs::metadata(&dest).map(|m| m.len() > 0).unwrap_or(false) {
            continue;
        }
        let url = format!("https://huggingface.co/{spec}/resolve/main/{file}?download=true");
        let ok = std::process::Command::new("curl")
            .args(["-sSL", "--fail", "--max-time", "1800", "-o"])
            .arg(&dest)
            .arg(&url)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return None;
        }
    }
    Some(dir)
}
