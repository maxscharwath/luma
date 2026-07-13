//! Dependency-free hashed term-frequency embedder (the default backend).
//!
//! Pipeline: tokenize → hash each token into one of `dim` buckets (the "hashing
//! trick", so there's no vocabulary to store) → sublinear term-frequency weight →
//! L2-normalize. No model, no new crates it compiles on the pinned 1.81 / musl
//! build as-is. Similarity reflects shared genres / cast / words, which is enough
//! for "more like this"; switch on `semantic` for free-text semantics.

use super::Embedder;

/// Hashed bag-of-words embedder. `dim` trades collisions for size/speed; 256 is
/// plenty for a few-thousand-title library.
pub struct LexicalEmbedder {
    dim: usize,
}

impl LexicalEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(16) }
    }
}

impl Embedder for LexicalEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn relevance_floor(&self) -> f32 {
        // Above the "shared generic token" noise floor; calibrated on a ~1k library.
        0.16
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for tok in tokenize(text) {
            if is_stopword(&tok) {
                continue;
            }
            let bucket = (fnv1a(&tok) % self.dim as u64) as usize;
            v[bucket] += 1.0;
        }
        // Sublinear term frequency: a word seen 10× shouldn't swamp one seen once.
        for x in v.iter_mut() {
            if *x > 0.0 {
                *x = 1.0 + x.ln();
            }
        }
        super::l2_normalize(&mut v);
        v
    }
}

/// Lowercase, split on non-alphanumerics, drop 1-char noise.
fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_lowercase())
}

/// FNV-1a 64-bit fast, allocation-free, good enough spread for bucketing.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A tiny stop-list so the most common English/French filler doesn't dominate the
/// overview text. Not exhaustive by design collisions wash out the long tail.
fn is_stopword(t: &str) -> bool {
    matches!(
        t,
        "the" | "and" | "for" | "with" | "from" | "that" | "this" | "his" | "her"
            | "its" | "their" | "are" | "was" | "but" | "not" | "you" | "who"
            | "les" | "des" | "une" | "dans" | "pour" | "par" | "qui" | "que"
            | "sur" | "est" | "son" | "ses" | "aux" | "avec" | "leur"
            // Generic film words: they appear in nearly every doc *and* in the
            // themed query phrases, inflating similarity to a baseline that buried
            // real signal (e.g. "christmas movie" matched anything via "movie").
            | "movie" | "movies" | "film" | "films" | "cinema" | "story"
    )
}
