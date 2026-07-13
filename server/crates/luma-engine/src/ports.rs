//! Ports: the abstract capabilities core services consume, implemented by
//! adapter modules and wired in by the composition root (the binary). Keeps the
//! engine free of any concrete module crate.
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Text -> unit vector embedder (implemented by the vector module).
pub trait Embedder: Send + Sync {
    /// Output dimension (stable for one embedder's lifetime).
    #[allow(dead_code)]
    fn dim(&self) -> usize;
    /// Embed `text` into a unit-length vector of length `dim`.
    fn embed(&self, text: &str) -> Vec<f32>;
    /// Minimum cosine for a themed-query hit to count as signal (backend-specific).
    fn relevance_floor(&self) -> f32;
}

/// A no-op fallback used when no embedder is injected (module absent): empty
/// vectors, so semantic features degrade to "no hits" rather than panicking.
pub struct NoopEmbedder;
impl Embedder for NoopEmbedder {
    fn dim(&self) -> usize { 0 }
    fn embed(&self, _text: &str) -> Vec<f32> { Vec::new() }
    fn relevance_floor(&self) -> f32 { 1.0 }
}

/// Audio -> text transcription (implemented by the whisper module).
pub trait Whisper: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn transcribe(
        &self,
        data_dir: &Path,
        model_spec: &str,
        input: &Path,
        track: u32,
        lang: Option<&str>,
        on_stage: &dyn Fn(&str),
        on_progress: &dyn Fn(usize, usize),
        cancel: &AtomicBool,
    ) -> Option<String>;
}
