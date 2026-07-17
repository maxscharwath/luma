//! The tantivy schema + the shared KROMA text analyzer.
//!
//! Every text field is tokenized by the `kroma` analyzer (lowercase + diacritic
//! fold), so "amelie" matches "Amélie" and casing never matters. Field ids are
//! stable across indexes built from the same [`Schema`], so a single [`Fields`]
//! is captured once and reused for every rebuild.

use tantivy::schema::{
    Field, IndexRecordOption, Schema, SchemaBuilder, TextFieldIndexing, TextOptions, STORED, STRING,
};
use tantivy::tokenizer::{
    AsciiFoldingFilter, LowerCaser, RemoveLongFilter, SimpleTokenizer, TextAnalyzer,
};
use tantivy::Index;

/// Name the KROMA analyzer is registered under on every index.
pub(super) const ANALYZER: &str = "kroma";

/// Handles to each schema field, cheap to copy and valid for any index built
/// from the schema returned by [`build`].
#[derive(Clone, Copy)]
pub(super) struct Fields {
    pub id: Field,
    pub kind: Field,
    pub title: Field,
    pub alt_title: Field,
    pub show_title: Field,
    pub cast: Field,
    pub genres: Field,
    pub overview: Field,
}

/// A `kroma`-analyzed, indexed (but not stored) text field.
fn text(b: &mut SchemaBuilder, name: &str) -> Field {
    let indexing = TextFieldIndexing::default()
        .set_tokenizer(ANALYZER)
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    b.add_text_field(name, TextOptions::default().set_indexing_options(indexing))
}

/// Build the schema and its field handles.
pub(super) fn build() -> (Schema, Fields) {
    let mut b = Schema::builder();
    // `id`/`kind` are stored verbatim so a hit can be mapped back to a DB row;
    // they're not analyzed (exact tokens, no fuzzy matching on them).
    let id = b.add_text_field("id", STRING | STORED);
    let kind = b.add_text_field("kind", STRING | STORED);
    let title = text(&mut b, "title");
    let alt_title = text(&mut b, "alt_title");
    let show_title = text(&mut b, "show_title");
    let cast = text(&mut b, "cast");
    let genres = text(&mut b, "genres");
    let overview = text(&mut b, "overview");
    let schema = b.build();
    let fields = Fields { id, kind, title, alt_title, show_title, cast, genres, overview };
    (schema, fields)
}

/// Create a fresh in-RAM index from `schema` with the `kroma` analyzer registered.
pub(super) fn new_index(schema: Schema) -> Index {
    let index = Index::create_in_ram(schema);
    let analyzer = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(40)) // ignore pathological "words"
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .build();
    index.tokenizers().register(ANALYZER, analyzer);
    index
}
