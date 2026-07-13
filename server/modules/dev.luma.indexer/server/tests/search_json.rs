//! Offline JSON search: parse a crafted JSON body against the clean-room JSON
//! fixture (mirrors `response: type: json` trackers).

use std::collections::HashMap;

use luma_indexer::{definition, engine, IndexerConfig, Query};

const JSON_DEF: &str = include_str!("fixtures/synthetic-json.yml");

fn config() -> IndexerConfig {
    IndexerConfig { base_url: "https://json.example/".to_string(), settings: HashMap::new() }
}

#[test]
fn builds_json_request() {
    let def = definition::parse(JSON_DEF.as_bytes()).unwrap();
    let reqs = engine::build_requests(&def, &config(), &Query::Text { query: "dune".into() }, &[2000]);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].url, "https://json.example/api/search?query=dune");
    assert_eq!(reqs[0].response_kind, "json");
}

#[test]
fn parses_json_rows() {
    let def = definition::parse(JSON_DEF.as_bytes()).unwrap();
    let cfg = config();
    let body = r#"
      {
        "total": 2,
        "results": [
          {
            "id": 501,
            "name": "Dune Part Two 2024 2160p",
            "category_id": 100,
            "info_hash": "abc123",
            "size_bytes": 34359738368,
            "seeders": 900,
            "leechers": 12
          },
          {
            "id": 502,
            "name": "Some Series S02 1080p",
            "category_id": 200,
            "info_hash": "def456",
            "size_bytes": 5368709120,
            "seeders": 40,
            "leechers": 3
          }
        ]
      }
    "#;

    let releases = engine::parse_json(&def, &cfg, body).unwrap();
    assert_eq!(releases.len(), 2);

    let first = &releases[0];
    assert_eq!(first.title, "Dune Part Two 2024 2160p");
    // `details`/`download` text templates reference the `_id` field.
    assert_eq!(first.details_url.as_deref(), Some("https://json.example/torrent/501"));
    assert_eq!(first.link.as_deref(), Some("https://json.example/download/501.torrent"));
    assert_eq!(first.info_hash.as_deref(), Some("abc123"));
    assert_eq!(first.size_bytes, Some(34_359_738_368));
    assert_eq!(first.seeders, Some(900));
    assert_eq!(first.categories, vec![2000]); // tracker 100 -> Movies

    assert_eq!(releases[1].categories, vec![5000]); // tracker 200 -> TV
}
