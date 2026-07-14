//! Content embeddings: turn a title's metadata into a dense, L2-normalized
//! vector so we can rank by "feels like this" powering similar-to, themed rows
//! and a personalized "For You" centroid (see `crate::db::vectors`).
//!
//! Two backends sit behind one [`Embedder`] trait:
//!   * [`LexicalEmbedder`] dependency-free hashed term vector. The DEFAULT:
//!     compiles on the pinned rustc 1.81 / musl build with zero new crates.
//!     Similarity reflects shared genres/cast/words. Good for "more like this".
//!   * `MiniLmEmbedder` a real `all-MiniLM-L6-v2` sentence-transformer via
//!     `candle` (Cargo feature `semantic`). Gives free-text *semantic*
//!     matches (e.g. embed the phrase "cozy christmas movie" and retrieve titles
//!     whose overview never says "christmas"). Heavier dep graph; opt-in.
//!
//! Both backends consume the SAME document string from [`build_doc`] and return a
//! normalized vector, so storage/search downstream is backend-agnostic.

use std::sync::Arc;

mod lexical;
pub use lexical::LexicalEmbedder;

// The out-of-process provider routes (the `.lmod`'s embedder-over-HTTP surface).
mod serve;
pub use serve::embedder_routes;

#[cfg(feature = "semantic")]
mod candle;

/// Produces a fixed-dimension, **L2-normalized** vector for a text document.
/// Normalization is part of the contract: it lets the search layer treat cosine
/// similarity as a plain dot product.
pub trait Embedder: Send + Sync {
    /// Output dimension (stable for the lifetime of one embedder). Used by the
    /// `semantic` backend (zero-vector fallback / storage sizing).
    #[allow(dead_code)]
    fn dim(&self) -> usize;
    /// Embed `text` into a unit-length vector of length [`dim`](Self::dim).
    fn embed(&self, text: &str) -> Vec<f32>;
    /// Minimum cosine for a hit to count as "really about" a themed query
    /// below this a row is just noise and the generator drops it. Backend-
    /// specific: lexical (sparse hashed TF) scores run lower than MiniLM's dense
    /// embeddings. Tunable against live probes.
    fn relevance_floor(&self) -> f32;
}

/// Pick the compiled-in backend: MiniLM when the `semantic` feature is
/// on (falling back to lexical if the model files can't be loaded), else the
/// dependency-free lexical embedder.
///
/// Cheap for the lexical path; the MiniLM path loads a ~25 MB model, so callers
/// should build this ONCE (e.g. into shared state) rather than per request. The
/// current prototype builds it once per enrichment pass.
pub fn default_embedder() -> Arc<dyn Embedder> {
    #[cfg(feature = "semantic")]
    {
        match candle::MiniLmEmbedder::load() {
            Ok(e) => return Arc::new(e),
            Err(err) => tracing::warn!(
                error = %err,
                "MiniLM load failed; falling back to the lexical embedder"
            ),
        }
    }
    Arc::new(LexicalEmbedder::new(256))
}

/// L2-normalize in place; a no-op for the zero vector. Shared by both backends
/// (a private parent item, visible to the `lexical`/`candle` submodules).
fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

pub mod module;
pub use module::MODULE;
