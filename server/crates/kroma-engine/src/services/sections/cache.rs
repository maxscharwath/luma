//! Process-wide in-memory snapshot of every title's embedding, so the section
//! generator ranks without re-reading + re-decoding SQLite on every request.
//!
//! Self-healing: [`refresh_if_stale`] polls a cheap `MAX(updated_at)` stamp and
//! reloads only when the vectors actually changed (a re-embed / backend switch),
//! so we never have to hook the (concurrently-evolving) enrichment path.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use crate::db::{self, Pool};

type Snapshot = Arc<Vec<(String, Vec<f32>)>>;

pub struct VectorCache {
    snap: RwLock<Snapshot>,
    /// Last-seen `MAX(item_vectors.updated_at)`; `None` until first load.
    stamp: RwLock<Option<String>>,
}

impl VectorCache {
    pub fn new() -> Self {
        Self { snap: RwLock::new(Arc::new(Vec::new())), stamp: RwLock::new(None) }
    }

    /// Reload the full snapshot from SQLite if the vectors changed since last load
    /// (or were never loaded). Cheap when unchanged: one indexed `MAX()` query.
    pub fn refresh_if_stale(&self, pool: &Pool) -> Result<()> {
        let current = db::vectors_max_updated_at(pool)?;
        let stale = { *self.stamp.read().unwrap() != current };
        if stale {
            let vectors = db::load_vectors(pool)?;
            *self.snap.write().unwrap() = Arc::new(vectors);
            *self.stamp.write().unwrap() = current;
        }
        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        self.snap.read().unwrap().clone()
    }

    /// Cloned `(id, vector)` pairs for `ids` present in the snapshot (order
    /// follows the snapshot, not `ids`). Powers per-user taste clustering.
    pub fn vectors_for(&self, ids: &[String]) -> Vec<(String, Vec<f32>)> {
        let want: HashSet<&str> = ids.iter().map(String::as_str).collect();
        self.snapshot()
            .iter()
            .filter(|(id, _)| want.contains(id.as_str()))
            .map(|(id, v)| (id.clone(), v.clone()))
            .collect()
    }

    /// Nearest `n` `(id, score)` to `query` by cosine (vectors are pre-normalized
    /// → dot), skipping `exclude` and any dimension-mismatched (stale) vector. The
    /// score lets the generator drop low-relevance rows (the noise floor).
    pub fn nearest(&self, query: &[f32], n: usize, exclude: &HashSet<&str>) -> Vec<(String, f32)> {
        let snap = self.snapshot();
        let mut scored: Vec<(String, f32)> = snap
            .iter()
            .filter(|(id, v)| v.len() == query.len() && !exclude.contains(id.as_str()))
            .map(|(id, v)| (id.clone(), dot(query, v)))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(n);
        scored
    }

    /// Nearest `n` `(id, score)` to a seed item (similar / "because you watched").
    /// Empty if the seed has no stored vector.
    pub fn similar(&self, id: &str, n: usize) -> Vec<(String, f32)> {
        let snap = self.snapshot();
        let Some((_, seed)) = snap.iter().find(|(vid, _)| vid == id) else {
            return Vec::new();
        };
        let seed = seed.clone();
        let exclude: HashSet<&str> = std::iter::once(id).collect();
        self.nearest(&seed, n, &exclude)
    }

    /// Taste centroid of `watched`, then the nearest `n` *unwatched* `(id, score)`.
    pub fn for_you(&self, watched: &[String], n: usize) -> Vec<(String, f32)> {
        let snap = self.snapshot();
        let want: HashSet<&str> = watched.iter().map(String::as_str).collect();
        let mut sum: Vec<f32> = Vec::new();
        let mut count = 0usize;
        for (id, v) in snap.iter() {
            if !want.contains(id.as_str()) {
                continue;
            }
            if sum.is_empty() {
                sum = vec![0.0; v.len()];
            }
            if sum.len() == v.len() {
                for (s, x) in sum.iter_mut().zip(v) {
                    *s += x;
                }
                count += 1;
            }
        }
        if count == 0 {
            return Vec::new();
        }
        normalize(&mut sum);
        self.nearest(&sum, n, &want)
    }
}

impl Default for VectorCache {
    fn default() -> Self {
        Self::new()
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
