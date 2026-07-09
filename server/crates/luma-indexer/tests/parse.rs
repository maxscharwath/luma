//! Schema round-trip tests against clean-room Cardigann fixtures.

use luma_indexer::{definition, Caps};

const HTML: &str = include_str!("fixtures/synthetic-html.yml");
const JSON: &str = include_str!("fixtures/synthetic-json.yml");

#[test]
fn parses_html_definition() {
    let def = definition::parse(HTML.as_bytes()).expect("parse html def");
    assert_eq!(def.id, "synthetic-html");
    assert_eq!(def.kind, "private");
    assert_eq!(def.request_delay, Some(2.0));
    assert_eq!(def.links, vec!["https://tracker.example/"]);

    // Category mappings normalize the int id to a string.
    assert_eq!(def.caps.categorymappings.len(), 3);
    assert_eq!(def.caps.categorymappings[0].id, "1");
    assert_eq!(def.caps.categorymappings[0].cat, "Movies/HD");

    // Settings: a select carries its option map + default.
    let sort = def.settings.iter().find(|s| s.name == "sort").unwrap();
    assert_eq!(sort.kind, "select");
    assert_eq!(sort.default.as_deref(), Some("seeders"));
    assert_eq!(sort.options.get("added").map(String::as_str), Some("Added"));

    // Login block.
    let login = def.login.as_ref().unwrap();
    assert_eq!(login.method.as_deref(), Some("form"));
    assert_eq!(login.form.as_deref(), Some("form#login"));
    assert_eq!(&*login.inputs["username"], "{{ .Config.username }}");
    assert_eq!(login.test.as_ref().unwrap().selector.as_deref(), Some("a[href=\"/logout.php\"]"));

    // Search rows + fields (order preserved; a field references filters).
    assert_eq!(def.search.rows.selector.as_deref(), Some("table.results tr.torrent"));
    let cat = &def.search.fields["category"];
    assert_eq!(cat.attribute.as_deref(), Some("href"));
    assert_eq!(cat.filters[0].name, "regexp");
    assert_eq!(cat.filters[0].args, vec!["cat=(\\d+)"]);
    // A `case` switch field.
    let dvf = &def.search.fields["downloadvolumefactor"];
    assert_eq!(dvf.case.get("span.freeleech").map(String::as_str), Some("0"));
    assert_eq!(dvf.case.get("*").map(String::as_str), Some("1"));

    // Caps derived from modes.
    let caps = Caps::from_definition(&def);
    assert!(caps.search_tmdb, "movie-search advertises tmdbid");
    assert!(caps.search_imdb);
    assert!(caps.tv_search_season);
    assert!(!caps.tv_search_tmdb);
    assert_eq!(caps.server_title.as_deref(), Some("Synthetic HTML Tracker"));
}

#[test]
fn parses_json_definition() {
    let def = definition::parse(JSON.as_bytes()).expect("parse json def");
    assert_eq!(def.id, "synthetic-json");
    assert_eq!(def.kind, "public");
    assert!(def.login.is_none());

    let path = &def.search.paths[0];
    assert_eq!(path.response.as_ref().unwrap().kind, "json");
    assert_eq!(def.search.rows.selector.as_deref(), Some("results"));
    assert_eq!(def.search.rows.count.as_ref().unwrap().selector.as_deref(), Some("total"));

    // A `text` template field that references an earlier `_id` field.
    let details = &def.search.fields["details"];
    assert_eq!(details.text.as_deref(), Some("/torrent/{{ .Result._id }}"));
}
