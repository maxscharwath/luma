//! End-to-end offline search against the clean-room HTML fixture: build the
//! request from a query, then parse a crafted response body into releases.

use std::collections::HashMap;

use luma_indexer::{definition, engine, IndexerConfig, Query};

const HTML_DEF: &str = include_str!("fixtures/synthetic-html.yml");

fn config() -> IndexerConfig {
    let mut settings = HashMap::new();
    settings.insert("username".to_string(), "alice".to_string());
    settings.insert("password".to_string(), "secret".to_string());
    settings.insert("sort".to_string(), "seeders".to_string());
    IndexerConfig { base_url: "https://tracker.example/".to_string(), settings }
}

#[test]
fn builds_search_request_from_query() {
    let def = definition::parse(HTML_DEF.as_bytes()).unwrap();
    let cfg = config();
    let query = Query::Movie {
        tmdb_id: Some(603),
        imdb_id: Some("tt0133093".into()),
        title: "The Matrix".into(),
        year: Some(1999),
    };
    let reqs = engine::build_requests(&def, &cfg, &query, &[2000, 5000]);
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    // Keywords templated in, sort from config, freeleech off -> no &fl=1.
    assert_eq!(r.url, "https://tracker.example/browse.php?q=The Matrix 1999&sort=seeders");
    assert_eq!(r.method, "get");
    assert_eq!(r.response_kind, "html");
}

#[test]
fn freeleech_toggle_changes_path() {
    let def = definition::parse(HTML_DEF.as_bytes()).unwrap();
    let mut cfg = config();
    cfg.settings.insert("freeleech".to_string(), "true".to_string());
    let query = Query::Text { query: "dune".into() };
    let reqs = engine::build_requests(&def, &cfg, &query, &[2000]);
    assert!(reqs[0].url.ends_with("&sort=seeders&fl=1"), "got {}", reqs[0].url);
}

#[test]
fn parses_rows_into_releases() {
    let def = definition::parse(HTML_DEF.as_bytes()).unwrap();
    let cfg = config();
    // A crafted response body matching the fixture's selectors. The first row
    // carries a freeleech badge (case switch -> downloadvolumefactor 0).
    let body = r#"
      <html><body>
      <table class="results">
        <tr class="torrent">
          <td class="cat"><a href="/browse.php?cat=1">Movies HD</a></td>
          <td class="name"><a href="/details.php?id=10">The Matrix 1999 1080p</a></td>
          <td class="size">8.4 GB</td>
          <td class="seeds">120</td>
          <td class="leech">4</td>
          <td class="age">2 days ago</td>
          <td class="dl"><a class="download" href="/download.php?id=10">dl</a><span class="freeleech">FL</span></td>
        </tr>
        <tr class="torrent">
          <td class="cat"><a href="/browse.php?cat=2">TV HD</a></td>
          <td class="name"><a href="/details.php?id=11">Some Show S01 720p</a></td>
          <td class="size">2.1 GB</td>
          <td class="seeds">30</td>
          <td class="leech">1</td>
          <td class="age">3 hours ago</td>
          <td class="dl"><a class="download" href="/download.php?id=11">dl</a></td>
        </tr>
      </table>
      </body></html>
    "#;

    let releases = engine::parse_html(&def, &cfg, body).unwrap();
    assert_eq!(releases.len(), 2);

    let first = &releases[0];
    assert_eq!(first.title, "The Matrix 1999 1080p");
    assert_eq!(first.details_url.as_deref(), Some("https://tracker.example/details.php?id=10"));
    assert_eq!(first.link.as_deref(), Some("https://tracker.example/download.php?id=10"));
    assert_eq!(first.size_bytes, Some((8.4 * 1024.0 * 1024.0 * 1024.0) as u64));
    assert_eq!(first.seeders, Some(120));
    assert_eq!(first.leechers, Some(4));
    // Category cell href -> regexp cat=(\d+) -> tracker id "1" -> Movies/HD 2040.
    assert_eq!(first.categories, vec![2040]);
    // Freeleech badge present -> download volume factor 0.
    assert_eq!(first.download_volume_factor, Some(0.0));
    assert_eq!(first.upload_volume_factor, Some(1.0));
    // Relative date parsed to a timestamp.
    assert!(first.published_at.as_deref().unwrap().contains('T'));

    let second = &releases[1];
    assert_eq!(second.title, "Some Show S01 720p");
    assert_eq!(second.categories, vec![5040]);
    // No freeleech badge -> case default 1.
    assert_eq!(second.download_volume_factor, Some(1.0));
}
