//! Locale overlay for served catalog entities.
//!
//! During the transition the `metadata` blob still carries the household
//! (primary) language; these helpers overlay the request locale's translation
//! on top so each user sees the catalog in *their* language. The overlay only
//! touches the localized text (title/tagline/overview/genres/character names)
//! the invariant art/ids/people already on the blob are left untouched. Applied
//! at the API boundary, right before serialization, keyed off `Accept-Language`.
//!
//! Resolution falls back requested lang -> `en` -> any (see
//! [`super::translations::resolve_many`]); an entity with no stored translation
//! keeps its blob text, so this is always safe to call.

use super::translations::{self, TransData};
use super::*;

use kroma_domain::{CastMember, Kind, MediaItem, Metadata, Season, SectionItem, Show, ShowDetail};

/// Overlay `locale` onto a batch of items (movies/videos + episodes). Episodes
/// resolve under the `'episode'` subject kind, everything else under `'item'`.
pub fn overlay_items(pool: &Pool, items: &mut [MediaItem], locale: &str) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let conn = pool.get()?;
    let movie_ids: Vec<&str> =
        items.iter().filter(|i| i.kind != Kind::Episode).map(|i| i.id.as_str()).collect();
    let ep_ids: Vec<&str> =
        items.iter().filter(|i| i.kind == Kind::Episode).map(|i| i.id.as_str()).collect();
    let movie_tr = translations::resolve_many(&conn, metadata_core::ITEM, &movie_ids, locale)?;
    let ep_tr = translations::resolve_many(&conn, "episode", &ep_ids, locale)?;
    for item in items.iter_mut() {
        let table = if item.kind == Kind::Episode { &ep_tr } else { &movie_tr };
        if let Some(tr) = table.get(&item.id) {
            apply(item.metadata.as_mut(), tr);
        }
    }
    Ok(())
}

/// Overlay `locale` onto home-section items (a mix of movies and shows).
pub fn overlay_section_items(pool: &Pool, items: &mut [SectionItem], locale: &str) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let conn = pool.get()?;
    let movie_ids: Vec<&str> = items
        .iter()
        .filter_map(|s| match s {
            SectionItem::Movie { item } => Some(item.id.as_str()),
            SectionItem::Show { .. } => None,
        })
        .collect();
    let show_ids: Vec<&str> = items
        .iter()
        .filter_map(|s| match s {
            SectionItem::Show { show } => Some(show.id.as_str()),
            SectionItem::Movie { .. } => None,
        })
        .collect();
    let m_tr = translations::resolve_many(&conn, metadata_core::ITEM, &movie_ids, locale)?;
    let s_tr = translations::resolve_many(&conn, metadata_core::SHOW, &show_ids, locale)?;
    for it in items.iter_mut() {
        match it {
            SectionItem::Movie { item } => {
                if let Some(t) = m_tr.get(&item.id) {
                    apply(item.metadata.as_mut(), t);
                }
            }
            SectionItem::Show { show } => {
                if let Some(t) = s_tr.get(&show.id) {
                    apply_show(show, t);
                }
            }
        }
    }
    Ok(())
}

/// Overlay `locale` onto a batch of shows (their top-level metadata only).
pub fn overlay_shows(pool: &Pool, shows: &mut [Show], locale: &str) -> Result<()> {
    if shows.is_empty() {
        return Ok(());
    }
    let conn = pool.get()?;
    let ids: Vec<&str> = shows.iter().map(|s| s.id.as_str()).collect();
    let tr = translations::resolve_many(&conn, metadata_core::SHOW, &ids, locale)?;
    for show in shows.iter_mut() {
        if let Some(t) = tr.get(&show.id) {
            apply_show(show, t);
        }
    }
    Ok(())
}

/// Overlay `locale` onto a full show detail: the show, every episode of every
/// season, and each season's cast character names (`season_cast` translations
/// keyed `"{show_id}:{season}"`).
pub fn overlay_show_detail(pool: &Pool, detail: &mut ShowDetail, locale: &str) -> Result<()> {
    let conn = pool.get()?;
    // Show + episodes reuse the batch helpers over this one detail.
    if let Some(t) =
        translations::resolve_many(&conn, metadata_core::SHOW, &[detail.show.id.as_str()], locale)?
            .get(&detail.show.id)
    {
        apply_show(&mut detail.show, t);
    }
    let ep_ids: Vec<&str> =
        detail.seasons.iter().flat_map(|s| s.episodes.iter()).map(|e| e.id.as_str()).collect();
    let ep_tr = translations::resolve_many(&conn, "episode", &ep_ids, locale)?;
    for season in &mut detail.seasons {
        for ep in &mut season.episodes {
            if let Some(t) = ep_tr.get(&ep.id) {
                apply(ep.metadata.as_mut(), t);
            }
        }
        overlay_season_cast(&conn, &detail.show.id, season, locale)?;
    }
    Ok(())
}

/// Overlay one season's cast character names from its `season_cast` translation.
fn overlay_season_cast(conn: &Connection, show_id: &str, season: &mut Season, locale: &str) -> Result<()> {
    if season.cast.is_empty() {
        return Ok(());
    }
    let sc_id = format!("{show_id}:{}", season.number);
    if let Some(t) = translations::resolve_many(conn, "season_cast", &[sc_id.as_str()], locale)?.get(&sc_id) {
        apply_characters(&mut season.cast, &t.characters);
    }
    Ok(())
}

/// Overlay the localized text fields onto an item's metadata (no-op when the item
/// has no blob metadata yet, i.e. not enriched).
fn apply(meta: Option<&mut Metadata>, tr: &TransData) {
    let Some(meta) = meta else { return };
    if tr.title.is_some() {
        meta.title = tr.title.clone();
    }
    if tr.tagline.is_some() {
        meta.tagline = tr.tagline.clone();
    }
    if tr.overview.is_some() {
        meta.overview = tr.overview.clone();
    }
    if !tr.genres.is_empty() {
        meta.genres = tr.genres.clone();
    }
    apply_characters(&mut meta.cast, &tr.characters);
}

/// A show's metadata overlay (same fields; shows carry no per-title cast here).
fn apply_show(show: &mut Show, tr: &TransData) {
    apply(show.metadata.as_mut(), tr);
}

/// Overlay localized character names onto a cast list, aligned by index (the
/// translation was written in the same TMDB cast order the core was stored in).
fn apply_characters(cast: &mut [CastMember], characters: &[Option<String>]) {
    for (member, ch) in cast.iter_mut().zip(characters.iter()) {
        if ch.is_some() {
            member.character = ch.clone();
        }
    }
}
