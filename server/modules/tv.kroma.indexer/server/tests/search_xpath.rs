//! XPath extraction (only built when the `xpath` feature is enabled).
#![cfg(feature = "xpath")]

use std::collections::HashMap;

use kroma_indexer::{definition, engine, IndexerConfig};

const XPATH_DEF: &str = include_str!("fixtures/synthetic-xpath.yml");

#[test]
fn parses_xpath_rows() {
    let def = definition::parse(XPATH_DEF.as_bytes()).unwrap();
    assert!(engine::uses_xpath(&def), "definition should be detected as XPath");
    let cfg = IndexerConfig { base_url: "https://xp.example/".into(), settings: HashMap::new() };
    let body = r#"
      <html><body>
      <table class="results">
        <tr class="torrent">
          <td class="name"><a href="/details/1">Cool Movie 2020 1080p</a></td>
          <td class="dl"><a href="/dl/1.torrent">get</a></td>
          <td class="size">4.2 GB</td>
          <td class="seeds">88</td>
          <td class="leech">2</td>
        </tr>
      </table>
      </body></html>
    "#;
    let releases = engine::parse_html_auto(&def, &cfg, body).unwrap();
    assert_eq!(releases.len(), 1);
    let r = &releases[0];
    assert_eq!(r.title, "Cool Movie 2020 1080p");
    assert_eq!(r.details_url.as_deref(), Some("https://xp.example/details/1"));
    assert_eq!(r.link.as_deref(), Some("https://xp.example/dl/1.torrent"));
    assert_eq!(r.size_bytes, Some((4.2 * 1024.0 * 1024.0 * 1024.0) as u64));
    assert_eq!(r.seeders, Some(88));
    assert_eq!(r.leechers, Some(2));
}
