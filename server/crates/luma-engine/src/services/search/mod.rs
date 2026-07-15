//! In-RAM full-text search over the catalogue (movies, shows, episodes).
//!
//! A tantivy index living entirely in a `RamDirectory`, rebuilt from SQLite the
//! source of truth on every library change (startup, watcher re-sync, manual
//! rescan, and once more after TMDB enrichment lands cast/overview/genres). A
//! rebuild constructs a brand-new index and atomically swaps it in, so searches
//! never see a half-built index and there's nothing on disk to migrate.
//!
//! This is keyword/typo-tolerant title search distinct from the semantic
//! "more like this / For You" recommender in [`crate::db`] vectors, which ranks by
//! embedding similarity rather than matching words.

mod query;
mod schema;

use std::sync::{Arc, RwLock};

use anyhow::Result;
use tantivy::collector::TopDocs;
use tantivy::schema::{Field, Schema, Value};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};
use tracing::{info, warn};

use std::collections::HashMap;

use crate::db::translations::TransData;
use crate::db::{self, Pool};
use crate::model::{Kind, MediaItem, Metadata, Show};
use crate::state::SharedState;

use schema::{Fields, ANALYZER};

/// Which catalogue table a [`Hit`] points at.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Movie,
    Show,
    Episode,
}

/// One match the catalogue id and what it is. Hits are returned already sorted
/// by descending relevance, so the position is the rank.
pub struct Hit {
    pub id: String,
    pub kind: HitKind,
}

/// The live, queryable index. Replaced wholesale on each rebuild so readers hold
/// a consistent snapshot for the life of one search.
struct Active {
    reader: IndexReader,
    // Held so the in-RAM directory and its registered tokenizer outlive `reader`.
    _index: Index,
}

/// Process-wide search engine. Cheap to clone (`Arc` in [`crate::state`]).
pub struct SearchEngine {
    schema: Schema,
    fields: Fields,
    active: RwLock<Arc<Active>>,
}

impl SearchEngine {
    /// Build an empty engine. Searches return nothing until the first rebuild.
    pub fn new() -> Result<Self> {
        let (schema, fields) = schema::build();
        let empty = HashMap::new();
        let active =
            build_active(schema.clone(), &fields, &[], &[], &[], &empty, &empty, &empty)?;
        Ok(Self { schema, fields, active: RwLock::new(Arc::new(active)) })
    }

    /// Replace the index with a fresh one built from the given catalogue.
    /// Rebuild the index from explicit catalog slices (no translations). Used by
    /// the search unit tests; production reindexing goes through [`Self::reindex_from_db`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn rebuild(&self, movies: &[MediaItem], shows: &[Show], episodes: &[MediaItem]) -> Result<()> {
        let empty = HashMap::new();
        let active =
            build_active(self.schema.clone(), &self.fields, movies, shows, episodes, &empty, &empty, &empty)?;
        *self.active.write().unwrap() = Arc::new(active);
        Ok(())
    }

    /// Rebuild from the current DB contents (two table scans, no per-row I/O).
    pub fn reindex_from_db(&self, pool: &Pool) -> Result<()> {
        let (items, shows) = db::index_snapshot(pool)?;
        let (episodes, movies): (Vec<MediaItem>, Vec<MediaItem>) =
            items.into_iter().partition(|i| matches!(i.kind, Kind::Episode));
        // Pull every stored language so the index matches a query in any of them
        // (the "queryable by the indexer" half of the language cache).
        let tr_movies = db::translations::all_for_kind(pool, db::metadata_core::ITEM).unwrap_or_default();
        let tr_eps = db::translations::all_for_kind(pool, "episode").unwrap_or_default();
        let tr_shows = db::translations::all_for_kind(pool, db::metadata_core::SHOW).unwrap_or_default();
        let active = build_active(
            self.schema.clone(), &self.fields, &movies, &shows, &episodes, &tr_movies, &tr_eps,
            &tr_shows,
        )?;
        *self.active.write().unwrap() = Arc::new(active);
        Ok(())
    }

    /// Top-`limit` hits for `raw`, best first. Empty for a blank query.
    pub fn search(&self, raw: &str, limit: usize) -> Vec<Hit> {
        let active = self.active.read().unwrap().clone();
        let tokens = normalize(&active._index, raw);
        let Some(query) = query::build(&self.fields, &tokens) else {
            return Vec::new();
        };
        let searcher = active.reader.searcher();
        // tantivy 0.26 removed TopDocs' blanket Collector impl; `.order_by_score()`
        // yields the score-ordered collector (same `Vec<(Score, DocAddress)>` fruit).
        let Ok(top) = searcher.search(&query, &TopDocs::with_limit(limit.max(1)).order_by_score())
        else {
            return Vec::new();
        };
        let mut hits = Vec::with_capacity(top.len());
        for (_score, addr) in top {
            let Ok(doc) = searcher.doc::<TantivyDocument>(addr) else { continue };
            let id = field_str(&doc, self.fields.id);
            if id.is_empty() {
                continue;
            }
            let kind = match field_str(&doc, self.fields.kind).as_str() {
                "show" => HitKind::Show,
                "episode" => HitKind::Episode,
                _ => HitKind::Movie,
            };
            hits.push(Hit { id, kind });
        }
        hits
    }
}

/// Build a fresh index, add every document, commit, and open a reader.
#[allow(clippy::too_many_arguments)]
fn build_active(
    schema: Schema,
    fields: &Fields,
    movies: &[MediaItem],
    shows: &[Show],
    episodes: &[MediaItem],
    tr_movies: &HashMap<String, Vec<TransData>>,
    tr_eps: &HashMap<String, Vec<TransData>>,
    tr_shows: &HashMap<String, Vec<TransData>>,
) -> Result<Active> {
    let index = schema::new_index(schema);
    let mut writer: IndexWriter = index.writer_with_num_threads(1, 15_000_000)?;
    for m in movies {
        add_item(&mut writer, fields, m, "movie", tr_movies.get(&m.id));
    }
    for e in episodes {
        add_item(&mut writer, fields, e, "episode", tr_eps.get(&e.id));
    }
    for s in shows {
        add_show(&mut writer, fields, s, tr_shows.get(&s.id));
    }
    writer.commit()?;
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    reader.reload()?;
    Ok(Active { reader, _index: index })
}

fn add_item(writer: &mut IndexWriter, f: &Fields, item: &MediaItem, kind: &str, tr: Option<&Vec<TransData>>) {
    let mut doc = TantivyDocument::new();
    doc.add_text(f.id, &item.id);
    doc.add_text(f.kind, kind);
    doc.add_text(f.title, &item.title);
    if let Some(t) = &item.episode_title {
        doc.add_text(f.title, t); // episode titles are searched as titles too
    }
    if let Some(st) = &item.show_title {
        doc.add_text(f.show_title, st);
    }
    add_meta(&mut doc, f, &item.title, item.metadata.as_ref());
    add_translations(&mut doc, f, &item.title, tr);
    let _ = writer.add_document(doc);
}

fn add_show(writer: &mut IndexWriter, f: &Fields, show: &Show, tr: Option<&Vec<TransData>>) {
    let mut doc = TantivyDocument::new();
    doc.add_text(f.id, &show.id);
    doc.add_text(f.kind, "show");
    doc.add_text(f.title, &show.title);
    add_meta(&mut doc, f, &show.title, show.metadata.as_ref());
    add_translations(&mut doc, f, &show.title, tr);
    let _ = writer.add_document(doc);
}

/// Index every stored language's title/overview/genres so a search matches the
/// user's language regardless of the household enrichment language. Titles that
/// differ from the filename title go into `alt_title`; multi-valued fields let us
/// add one set per language to the same document.
fn add_translations(doc: &mut TantivyDocument, f: &Fields, file_title: &str, tr: Option<&Vec<TransData>>) {
    let Some(list) = tr else { return };
    for t in list {
        if let Some(title) = &t.title {
            if !title.eq_ignore_ascii_case(file_title) {
                doc.add_text(f.alt_title, title);
            }
        }
        if let Some(o) = &t.overview {
            doc.add_text(f.overview, o);
        }
        for g in &t.genres {
            doc.add_text(f.genres, g);
        }
    }
}

/// Index the searchable parts of TMDB metadata: a differing localized title,
/// overview, genres and cast names.
fn add_meta(doc: &mut TantivyDocument, f: &Fields, file_title: &str, meta: Option<&Metadata>) {
    let Some(meta) = meta else { return };
    if let Some(t) = &meta.title {
        if !t.eq_ignore_ascii_case(file_title) {
            doc.add_text(f.alt_title, t);
        }
    }
    if let Some(o) = &meta.overview {
        doc.add_text(f.overview, o);
    }
    for g in &meta.genres {
        doc.add_text(f.genres, g);
    }
    for c in &meta.cast {
        doc.add_text(f.cast, &c.name);
    }
}

/// Tokenize `raw` with the index's own analyzer, so query terms are lowercased +
/// diacritic-folded identically to the indexed terms.
fn normalize(index: &Index, raw: &str) -> Vec<String> {
    let Some(mut analyzer) = index.tokenizers().get(ANALYZER) else {
        return raw.split_whitespace().map(str::to_lowercase).collect();
    };
    let mut stream = analyzer.token_stream(raw);
    let mut tokens = Vec::new();
    while stream.advance() {
        tokens.push(stream.token().text.clone());
    }
    tokens
}

fn field_str(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Rebuild the search index from the DB on a detached thread. Never blocks the
/// caller; a failure is logged, not fatal (search just keeps the prior index).
pub fn spawn_reindex(state: SharedState) {
    std::thread::spawn(move || match state.search.reindex_from_db(&state.db) {
        Ok(()) => info!("search index rebuilt"),
        Err(e) => warn!(error = %e, "search reindex failed"),
    });
}

#[cfg(test)]
mod tests;
