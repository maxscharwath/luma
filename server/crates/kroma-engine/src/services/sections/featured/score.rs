//! The pure scoring half of the hero pick: the per-signal helpers, the weighted
//! blend they feed, and the deterministic daily rotation. Nothing here reads the
//! DB or the clock, so every rule is unit-testable on hand-built entries.

use std::collections::HashMap;

use crate::model::{Metadata, SectionItem, VideoStream};

/// Milliseconds in a day: the quantum for both the scoring clock and the
/// rotation (see the module docs on staying stable within a day).
pub(super) const DAY_MS: i64 = 86_400_000;
/// Freshness half-life, in days: a title keeps half its freshness this long.
const HALF_LIFE_DAYS: f64 = 14.0;

// Signal weights (sum to 1.0). Taste is the one signal a title can simply not
// *have* (nothing embedded it yet), so its weight joins the denominator per
// candidate rather than pool-wide; the others always count, because their zero
// is a real reading (unrated, ancient, never played, not 4K).
const W_TASTE: f32 = 0.35;
const W_QUALITY: f32 = 0.20;
const W_FRESH: f32 = 0.25;
const W_TREND: f32 = 0.10;
const W_CINEMA: f32 = 0.10;

/// Score `candidates` and return the best `top` of them, best first. Ties break
/// on id for determinism (the rotation indexes into this list, so its order must
/// not wobble between requests).
///
/// Only the head is ordered: the pool is a whole-catalog scan and the caller
/// keeps one entry, so this partitions with `select_nth_unstable_by` and sorts
/// the survivors, and it hands back borrows so nothing is cloned on the way.
pub(super) fn rank<'a>(
    candidates: &[&'a SectionItem],
    taste: &HashMap<String, f32>,
    trend: &HashMap<String, f32>,
    now_ms: i64,
    top: usize,
) -> Vec<&'a SectionItem> {
    let head = top.min(candidates.len());
    if head == 0 {
        return Vec::new();
    }
    // Min-max normalize the taste cosine across the pool: raw lexical cosines
    // barely separate titles, so spread them to a usable 0..1.
    let vals: Vec<f32> = candidates.iter().filter_map(|e| taste.get(e.id())).copied().collect();
    let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    let spread = hi - lo;
    let norm = (vals.len() > 1 && spread > f32::EPSILON).then_some((lo, spread));

    let mut scored: Vec<(&SectionItem, f32)> =
        candidates.iter().map(|e| (*e, blend(e, taste, trend, now_ms, norm))).collect();
    let by_rank = |a: &(&SectionItem, f32), b: &(&SectionItem, f32)| {
        b.1.total_cmp(&a.1).then_with(|| a.0.id().cmp(b.0.id()))
    };
    if head < scored.len() {
        scored.select_nth_unstable_by(head - 1, by_rank);
        scored.truncate(head);
    }
    scored.sort_by(by_rank);
    scored.into_iter().map(|(e, _)| e).collect()
}

/// One candidate's blended score. The weights are summed **per candidate**: a
/// signal this entry has no datum for leaves both sides of the ratio, instead of
/// scoring a zero that every other candidate is compared against (a title with
/// no embedding yet must not eat a flat `W_TASTE` haircut). `taste_norm` is the
/// pool's `(lo, spread)`, `None` when the pool has no usable spread to stretch.
fn blend(
    e: &SectionItem,
    taste: &HashMap<String, f32>,
    trend: &HashMap<String, f32>,
    now_ms: i64,
    taste_norm: Option<(f32, f32)>,
) -> f32 {
    let mut sum = W_QUALITY * quality(e)
        + W_FRESH * freshness(added_at(e), now_ms)
        + W_TREND * trend.get(e.id()).copied().unwrap_or(0.0)
        + W_CINEMA * cinematic(video(e));
    let mut weight = W_QUALITY + W_FRESH + W_TREND + W_CINEMA;
    if let (Some((lo, spread)), Some(v)) = (taste_norm, taste.get(e.id())) {
        sum += W_TASTE * (v - lo) / spread;
        weight += W_TASTE;
    }
    sum / weight
}

/// The hero needs art and a synopsis to render.
pub(super) fn presentable(e: &SectionItem) -> bool {
    meta(e).is_some_and(|m| {
        m.backdrop_url.is_some() && m.overview.as_deref().is_some_and(|o| !o.is_empty())
    })
}

/// TMDB rating scaled to 0..1; unrated titles sit at a neutral 0.5.
fn quality(e: &SectionItem) -> f32 {
    meta(e).and_then(|m| m.rating).map_or(0.5, |r| (r / 10.0).clamp(0.0, 1.0))
}

/// `0.5 ^ (age / half-life)`: 1.0 when just added, 0.5 after two weeks, ~0 for
/// back-catalog. Unparseable stamps read as ancient (0.0).
fn freshness(added_at: &str, now_ms: i64) -> f32 {
    let format = time::format_description::well_known::Rfc3339;
    let Ok(ts) = time::OffsetDateTime::parse(added_at, &format) else {
        return 0.0;
    };
    let age_days = (now_ms as f64 / 1000.0 - ts.unix_timestamp() as f64) / 86_400.0;
    (0.5f64.powf(age_days / HALF_LIFE_DAYS).min(1.0)) as f32
}

/// Showcase bonus: 4K counts more than HDR; both stack to 1.0.
fn cinematic(v: Option<&VideoStream>) -> f32 {
    let Some(v) = v else { return 0.0 };
    let mut score = 0.0;
    if v.width.unwrap_or(0) >= 3840 {
        score += 0.6;
    }
    if v.hdr {
        score += 0.4;
    }
    score
}

/// Deterministic daily rotation slot: same user + same day -> same index, next
/// day -> the next of the top `k`; the user hash de-syncs household members.
pub(super) fn rotation_index(day: u64, user_id: &str, k: usize) -> usize {
    (day.wrapping_add(fnv1a(user_id)) % k.max(1) as u64) as usize
}

/// FNV-1a 64: tiny, stable, good enough to spread users across rotation slots.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn meta(e: &SectionItem) -> Option<&Metadata> {
    match e {
        SectionItem::Movie { item } => item.metadata.as_ref(),
        SectionItem::Show { show } => show.metadata.as_ref(),
    }
}

fn video(e: &SectionItem) -> Option<&VideoStream> {
    match e {
        SectionItem::Movie { item } => item.video.as_ref(),
        SectionItem::Show { show } => show.video.as_ref(),
    }
}

fn added_at(e: &SectionItem) -> &str {
    match e {
        SectionItem::Movie { item } => &item.added_at,
        SectionItem::Show { show } => &show.added_at,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{iso, meta, movie, stream, NOW_MS};
    use super::*;

    /// Rank the whole pool (the tests assert on full orderings).
    fn ranked_ids(
        refs: &[&SectionItem],
        taste: &HashMap<String, f32>,
        trend: &HashMap<String, f32>,
    ) -> Vec<String> {
        rank(refs, taste, trend, NOW_MS, refs.len())
            .iter()
            .map(|e| e.id().to_string())
            .collect()
    }

    /// Two interchangeable titles: same rating, same age, no video, no trend.
    fn twin(id: &str) -> SectionItem {
        movie(id, Some(meta(Some(7.0), true, true)), &iso(30 * DAY_MS), None)
    }

    #[test]
    fn freshness_decays_with_half_life() {
        let now = freshness(&iso(0), NOW_MS);
        let two_weeks = freshness(&iso(14 * DAY_MS), NOW_MS);
        let stale = freshness(&iso(365 * DAY_MS), NOW_MS);
        assert!(now > 0.99);
        assert!((two_weeks - 0.5).abs() < 0.01);
        assert!(stale < 0.01);
        // Clock skew (future stamp) clamps to 1, garbage reads as ancient.
        assert_eq!(freshness(&iso(-DAY_MS), NOW_MS), 1.0);
        assert_eq!(freshness("t", NOW_MS), 0.0);
    }

    #[test]
    fn cinematic_rewards_4k_and_hdr() {
        assert_eq!(cinematic(None), 0.0);
        assert_eq!(cinematic(Some(&stream(1920, false))), 0.0);
        assert_eq!(cinematic(Some(&stream(3840, false))), 0.6);
        assert!((cinematic(Some(&stream(3840, true))) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn quality_is_neutral_when_unrated() {
        let rated = movie("a", Some(meta(Some(8.0), true, true)), "t", None);
        let unrated = movie("b", Some(meta(None, true, true)), "t", None);
        assert!((quality(&rated) - 0.8).abs() < f32::EPSILON);
        assert!((quality(&unrated) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn presentable_requires_backdrop_and_overview() {
        assert!(presentable(&movie("a", Some(meta(None, true, true)), "t", None)));
        assert!(!presentable(&movie("b", Some(meta(None, false, true)), "t", None)));
        assert!(!presentable(&movie("c", Some(meta(None, true, false)), "t", None)));
        assert!(!presentable(&movie("d", None, "t", None)));
    }

    #[test]
    fn rotation_is_deterministic_and_bounded() {
        let k = 5;
        let a = rotation_index(100, "user-1", k);
        assert_eq!(a, rotation_index(100, "user-1", k));
        assert!(a < k);
        // The next day advances the slot by exactly one.
        assert_eq!(rotation_index(101, "user-1", k), (a + 1) % k);
        // k = 0 is guarded (empty pools never reach here, but stay safe).
        assert_eq!(rotation_index(100, "user-1", 0), 0);
    }

    #[test]
    fn rank_prefers_fresh_high_quality_cinematic() {
        let winner =
            movie("new", Some(meta(Some(8.5), true, true)), &iso(DAY_MS), Some(stream(3840, true)));
        let loser = movie("old", Some(meta(Some(5.0), true, true)), &iso(400 * DAY_MS), None);
        let refs: Vec<&SectionItem> = vec![&loser, &winner];
        assert_eq!(ranked_ids(&refs, &HashMap::new(), &HashMap::new())[0], "new");
    }

    #[test]
    fn rank_uses_taste_to_separate_equal_titles() {
        let (a, b) = (twin("a"), twin("b"));
        let refs: Vec<&SectionItem> = vec![&a, &b];
        let taste: HashMap<String, f32> = [("a".to_string(), 0.31), ("b".to_string(), 0.62)].into();
        assert_eq!(ranked_ids(&refs, &taste, &HashMap::new())[0], "b");
        // Without taste the tie breaks on id instead.
        assert_eq!(ranked_ids(&refs, &HashMap::new(), &HashMap::new())[0], "a");
    }

    #[test]
    fn rank_renormalizes_taste_per_candidate() {
        // Same title three times over; only a and c have been embedded.
        let (a, b, c) = (twin("a"), twin("b"), twin("c"));
        let refs: Vec<&SectionItem> = vec![&a, &b, &c];
        let taste: HashMap<String, f32> = [("a".to_string(), 0.1), ("c".to_string(), 0.9)].into();
        // c wins on taste; b, which the embedder has no vector for, is judged on
        // the signals it *does* have rather than taking a flat W_TASTE haircut,
        // so it lands above a (whose taste normalizes to the pool floor, 0).
        assert_eq!(ranked_ids(&refs, &taste, &HashMap::new()), ["c", "b", "a"]);
    }

    #[test]
    fn rank_uses_trending_signal() {
        let (a, b) = (twin("a"), twin("b"));
        let refs: Vec<&SectionItem> = vec![&a, &b];
        let trend: HashMap<String, f32> = [("b".to_string(), 1.0)].into();
        assert_eq!(ranked_ids(&refs, &HashMap::new(), &trend)[0], "b");
    }

    #[test]
    fn rank_returns_only_the_requested_head() {
        let (a, b, c) = (twin("a"), twin("b"), twin("c"));
        let refs: Vec<&SectionItem> = vec![&c, &b, &a];
        // The partial sort keeps the same winners the full sort would, in order.
        let head = rank(&refs, &HashMap::new(), &HashMap::new(), NOW_MS, 2);
        assert_eq!(head.iter().map(|e| e.id()).collect::<Vec<_>>(), ["a", "b"]);
        // Asking for more than the pool holds is capped, asking for none is empty.
        assert_eq!(rank(&refs, &HashMap::new(), &HashMap::new(), NOW_MS, 99).len(), 3);
        assert!(rank(&refs, &HashMap::new(), &HashMap::new(), NOW_MS, 0).is_empty());
    }
}
