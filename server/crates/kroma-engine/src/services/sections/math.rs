//! Small vector helpers shared by the embedding-based section code: the vector
//! cache's nearest-neighbour search ([`super::cache`]) and the taste clusterer
//! ([`super::taste`]). Both operate on pre-normalized embedding vectors.

/// Dot product of two equal-length vectors. On pre-normalized vectors this is the
/// cosine similarity.
pub(super) fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Scale `v` to unit length in place. A no-op on the zero vector.
pub(super) fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
