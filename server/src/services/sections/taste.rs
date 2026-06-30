//! Deterministic taste modelling: cluster a user's watch history in embedding
//! space into a few coherent "taste groups", each summarized by its example
//! titles + dominant genres/keywords. This is the *understanding* half of the
//! smart-sections feature it sharpens automatically as history grows. The LLM
//! then only has to *name* each cluster (see the `sections.personalize` job),
//! which keeps the model small and the catalog items real (no hallucination).

use std::collections::HashMap;

use crate::db::{self, Pool};
use crate::services::sections::VectorCache;

/// Minimum distinct watched titles (with embeddings) before clustering is worth
/// it below this the home falls back to the static themed bank.
pub const MIN_WATCHED: usize = 5;

/// One taste group.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// Member item ids, nearest-to-centroid first.
    pub ids: Vec<String>,
    /// Example titles (nearest-first), for the LLM prompt.
    pub titles: Vec<String>,
    /// Dominant genres across members (most common first).
    pub genres: Vec<String>,
    /// Dominant keyword tags across members (most common first).
    pub keywords: Vec<String>,
}

/// Cluster `watched` into at most `k` taste groups. Empty when the user has too
/// little embeddable history. Pure + deterministic (fixed seeding) so re-runs are
/// stable.
pub fn cluster(pool: &Pool, vectors: &VectorCache, watched: &[String], k: usize) -> Vec<Cluster> {
    let mut vecs = vectors.vectors_for(watched);
    if vecs.len() < MIN_WATCHED {
        return Vec::new();
    }
    // Keep only the dominant dimension (a stale vector from a backend switch
    // would otherwise poison the centroids).
    let dim = vecs.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
    vecs.retain(|(_, v)| v.len() == dim && dim > 0);
    if vecs.len() < MIN_WATCHED {
        return Vec::new();
    }

    let k = k.clamp(1, vecs.len());
    let assignments = kmeans(&vecs, k);

    // Group member indices by cluster.
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); k];
    for (i, &c) in assignments.iter().enumerate() {
        groups[c].push(i);
    }

    // Resolve metadata once for every member.
    let all_ids: Vec<&str> = vecs.iter().map(|(id, _)| id.as_str()).collect();
    let items = db::items_by_ids(pool, &all_ids).unwrap_or_default();
    let meta: HashMap<&str, &crate::model::MediaItem> = items.iter().map(|it| (it.id.as_str(), it)).collect();

    let mut clusters = Vec::new();
    for members in groups {
        if members.is_empty() {
            continue;
        }
        // Centroid + order members nearest-first for representative picks.
        let centroid = mean(&members.iter().map(|&i| vecs[i].1.as_slice()).collect::<Vec<_>>());
        let mut ranked: Vec<(usize, f32)> =
            members.iter().map(|&i| (i, dot(&centroid, &vecs[i].1))).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));

        let ids: Vec<String> = ranked.iter().map(|&(i, _)| vecs[i].0.clone()).collect();
        let titles: Vec<String> = ids
            .iter()
            .filter_map(|id| meta.get(id.as_str()).map(|it| it.title.clone()))
            .take(6)
            .collect();
        let (genres, keywords) = aggregate_tags(&ids, &meta);
        clusters.push(Cluster { ids, titles, genres, keywords });
    }
    // Biggest taste groups first.
    clusters.sort_by(|a, b| b.ids.len().cmp(&a.ids.len()));
    clusters
}

/// Tally genres + keywords across a cluster's members; most-common first.
fn aggregate_tags(ids: &[String], meta: &HashMap<&str, &crate::model::MediaItem>) -> (Vec<String>, Vec<String>) {
    let mut genres: HashMap<String, usize> = HashMap::new();
    let mut keywords: HashMap<String, usize> = HashMap::new();
    for id in ids {
        if let Some(m) = meta.get(id.as_str()).and_then(|it| it.metadata.as_ref()) {
            for g in &m.genres {
                *genres.entry(g.clone()).or_default() += 1;
            }
            for k in &m.keywords {
                *keywords.entry(k.clone()).or_default() += 1;
            }
        }
    }
    (top_n(genres, 5), top_n(keywords, 8))
}

fn top_n(counts: HashMap<String, usize>, n: usize) -> Vec<String> {
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    // Count desc, then name for a stable tiebreak.
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|(k, _)| k).collect()
}

// ----- k-means (cosine / dot on pre-normalized vectors) -----------------------

const KMEANS_ITERS: usize = 12;

/// Assign each vector to one of `k` clusters. Deterministic seeding (evenly
/// spaced picks) so the same history yields the same grouping run to run.
fn kmeans(vecs: &[(String, Vec<f32>)], k: usize) -> Vec<usize> {
    let n = vecs.len();
    let dim = vecs[0].1.len();
    let mut centroids: Vec<Vec<f32>> =
        (0..k).map(|c| vecs[(c * n) / k].1.clone()).collect();

    let mut assign = vec![0usize; n];
    for _ in 0..KMEANS_ITERS {
        let mut changed = false;
        for (i, (_, v)) in vecs.iter().enumerate() {
            let best = (0..k)
                .max_by(|&a, &b| dot(&centroids[a], v).total_cmp(&dot(&centroids[b], v)))
                .unwrap_or(0);
            if best != assign[i] {
                assign[i] = best;
                changed = true;
            }
        }
        // Recompute centroids as the (normalized) mean of their members.
        let mut sums = vec![vec![0.0f32; dim]; k];
        let mut counts = vec![0usize; k];
        for (i, (_, v)) in vecs.iter().enumerate() {
            let c = assign[i];
            counts[c] += 1;
            for (s, x) in sums[c].iter_mut().zip(v) {
                *s += x;
            }
        }
        for c in 0..k {
            if counts[c] > 0 {
                for s in &mut sums[c] {
                    *s /= counts[c] as f32;
                }
                normalize(&mut sums[c]);
                centroids[c] = std::mem::take(&mut sums[c]);
            }
        }
        if !changed {
            break;
        }
    }
    assign
}

fn mean(vectors: &[&[f32]]) -> Vec<f32> {
    let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
    let mut sum = vec![0.0f32; dim];
    for v in vectors {
        for (s, x) in sum.iter_mut().zip(*v) {
            *s += x;
        }
    }
    if !vectors.is_empty() {
        for s in &mut sum {
            *s /= vectors.len() as f32;
        }
    }
    normalize(&mut sum);
    sum
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

#[cfg(test)]
mod tests {
    use super::*;

    // Two well-separated blobs in 2-D → k-means must split them cleanly.
    #[test]
    fn kmeans_separates_two_blobs() {
        let mk = |x: f32, y: f32| {
            let mut v = vec![x, y];
            normalize(&mut v);
            v
        };
        let vecs: Vec<(String, Vec<f32>)> = vec![
            ("a".into(), mk(1.0, 0.05)),
            ("b".into(), mk(1.0, 0.0)),
            ("c".into(), mk(0.95, 0.1)),
            ("d".into(), mk(0.05, 1.0)),
            ("e".into(), mk(0.0, 1.0)),
            ("f".into(), mk(0.1, 0.95)),
        ];
        let assign = kmeans(&vecs, 2);
        // a,b,c share a cluster; d,e,f share the other.
        assert_eq!(assign[0], assign[1]);
        assert_eq!(assign[1], assign[2]);
        assert_eq!(assign[3], assign[4]);
        assert_eq!(assign[4], assign[5]);
        assert_ne!(assign[0], assign[3]);
    }

    #[test]
    fn top_n_orders_by_count() {
        let mut counts = HashMap::new();
        counts.insert("action".to_string(), 5);
        counts.insert("drama".to_string(), 2);
        counts.insert("comedy".to_string(), 8);
        assert_eq!(top_n(counts, 2), vec!["comedy", "action"]);
    }
}
