//! Pull-parsing of Torznab XML with quick-xml: the RSS search results (items
//! with `<torznab:attr>` extensions, CDATA titles, escaped entities) and the
//! `t=caps` capability document. Torznab error documents surface as `Err`.

use anyhow::{bail, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::{Caps, Release};

/// Parse a Torznab RSS search response into releases.
pub fn parse_items(xml: &[u8]) -> Result<Vec<Release>> {
    let mut reader = Reader::from_reader(xml);
    // trim_text off so a field split across text + entity-ref + CDATA events keeps
    // its internal spacing; `apply_field` trims the accumulated value's ends.
    reader.config_mut().trim_text(false);
    let decoder = reader.decoder();

    let mut out: Vec<Release> = Vec::new();
    let mut current: Option<Release> = None;
    // The simple text element being read inside an item, if any.
    let mut field: Option<&'static str> = None;
    // Accumulates the current field's text across Text / GeneralRef (entity) /
    // CDATA events (quick-xml 0.41 splits entities out of text as GeneralRef),
    // flushed to `apply_field` when the element closes.
    let mut text = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                text.clear();
                field = match e.local_name().as_ref() {
                    b"item" => {
                        current = Some(Release::default());
                        None
                    }
                    b"title" if current.is_some() => Some("title"),
                    b"guid" if current.is_some() => Some("guid"),
                    b"link" if current.is_some() => Some("link"),
                    b"comments" if current.is_some() => Some("comments"),
                    b"size" if current.is_some() => Some("size"),
                    b"pubDate" if current.is_some() => Some("pubDate"),
                    _ => None,
                };
            }
            Event::Empty(e) => match e.local_name().as_ref() {
                b"error" => {
                    let mut code = String::new();
                    let mut description = String::new();
                    for attr in e.attributes().flatten() {
                        let v = quick_xml::escape::unescape(&decoder.decode(&attr.value)?)?.into_owned();
                        match attr.key.local_name().as_ref() {
                            b"code" => code = v,
                            b"description" => description = v,
                            _ => {}
                        }
                    }
                    bail!("torznab error {code}: {description}");
                }
                b"enclosure" => {
                    if let Some(rel) = current.as_mut() {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"url" {
                                let url = quick_xml::escape::unescape(&decoder.decode(&attr.value)?)?.into_owned();
                                if rel.link.is_none() {
                                    rel.link = Some(url);
                                }
                            }
                        }
                    }
                }
                b"attr" => {
                    if let Some(rel) = current.as_mut() {
                        let (mut name, mut value) = (String::new(), String::new());
                        for attr in e.attributes().flatten() {
                            let v = quick_xml::escape::unescape(&decoder.decode(&attr.value)?)?.into_owned();
                            match attr.key.local_name().as_ref() {
                                b"name" => name = v,
                                b"value" => value = v,
                                _ => {}
                            }
                        }
                        apply_attr(rel, &name, &value);
                    }
                }
                _ => {}
            },
            // Text carries only literal text in 0.41 (no `&amp;` to unescape).
            Event::Text(e) if field.is_some() => text.push_str(&e.decode()?),
            // An entity reference inside a field (`&amp;`, `&#38;`, ...).
            Event::GeneralRef(r) if field.is_some() => text.push_str(&resolve_entity(&r)),
            Event::CData(e) if field.is_some() => {
                text.push_str(&String::from_utf8_lossy(&e));
            }
            Event::End(e) => {
                // Flush the accumulated field text as the element closes.
                if let (Some(rel), Some(f)) = (current.as_mut(), field.take()) {
                    apply_field(rel, f, &text);
                }
                text.clear();
                if e.local_name().as_ref() == b"item" {
                    if let Some(rel) = current.take() {
                        if !rel.title.is_empty() {
                            out.push(rel);
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// Resolve a quick-xml `GeneralRef` (the `amp` of `&amp;`, or `#38` of `&#38;`)
/// to its text by rebuilding the escaped form and running the standard XML
/// unescaper (predefined entities + numeric refs). Unknown refs drop to empty.
fn resolve_entity(r: &quick_xml::events::BytesRef) -> String {
    r.decode()
        .ok()
        .and_then(|name| {
            quick_xml::escape::unescape(&format!("&{name};")).ok().map(|c| c.into_owned())
        })
        .unwrap_or_default()
}

fn apply_field(rel: &mut Release, field: &str, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    match field {
        "title" => rel.title = text.to_string(),
        "guid" => {
            rel.guid = text.to_string();
            // Many trackers set <guid> to the details page URL; keep it as a
            // fallback info link when no explicit <comments> is provided.
            if text.starts_with("http") {
                rel.details_url.get_or_insert_with(|| text.to_string());
            }
        }
        // The tracker's human torrent page; wins over the guid fallback.
        "comments" if text.starts_with("http") => rel.details_url = Some(text.to_string()),
        // <link> can be the magnet itself on some indexers.
        "link" if text.starts_with("magnet:") => {
            rel.magnet.get_or_insert_with(|| text.to_string());
        }
        "link" => {
            rel.link.get_or_insert_with(|| text.to_string());
        }
        "size" => rel.size_bytes = text.parse().ok(),
        "pubDate" => rel.published_at = Some(text.to_string()),
        _ => {}
    }
}

fn apply_attr(rel: &mut Release, name: &str, value: &str) {
    match name {
        "seeders" => rel.seeders = value.parse().ok(),
        // Torznab "peers" is seeders + leechers.
        "peers" => {
            if let Ok(peers) = value.parse::<u32>() {
                rel.leechers = Some(peers.saturating_sub(rel.seeders.unwrap_or(0)));
            }
        }
        "infohash" => rel.info_hash = Some(value.to_string()),
        "magneturl" => rel.magnet = Some(value.to_string()),
        "imdbid" => rel.imdb_id = Some(value.to_string()),
        "tmdbid" => rel.tmdb_id = value.parse().ok(),
        "size" => rel.size_bytes = rel.size_bytes.or_else(|| value.parse().ok()),
        _ => {}
    }
}

/// Parse a `t=caps` document into the supported-parameter flags.
pub fn parse_caps(xml: &[u8]) -> Result<Caps> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let decoder = reader.decoder();
    let mut caps = Caps::default();
    let mut buf = Vec::new();

    loop {
        let event = reader.read_event_into(&mut buf)?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.local_name().as_ref().to_vec();
                let mut supported = String::new();
                for attr in e.attributes().flatten() {
                    let v = quick_xml::escape::unescape(&decoder.decode(&attr.value)?)?.into_owned();
                    match attr.key.local_name().as_ref() {
                        b"supportedParams" => supported = v,
                        b"title" if name == b"server" => caps.server_title = Some(v),
                        b"code" if name == b"error" => {
                            bail!("torznab error {v}");
                        }
                        _ => {}
                    }
                }
                let has = |p: &str| supported.split(',').any(|s| s.trim() == p);
                match name.as_slice() {
                    b"movie-search" => {
                        caps.search_tmdb = has("tmdbid");
                        caps.search_imdb = has("imdbid");
                    }
                    b"tv-search" => caps.tv_search_tmdb = has("tmdbid"),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(caps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape captured from a Jackett Torznab response: CDATA title, escaped
    /// entities in URLs, enclosure + torznab:attr extensions.
    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:torznab="http://torznab.com/schemas/2015/feed">
 <channel>
  <title>Indexer</title>
  <item>
   <title><![CDATA[The.Matrix.1999.1080p.BluRay.x265-GRP]]></title>
   <guid>https://tracker.example/details?id=42&amp;page=1</guid>
   <comments>https://tracker.example/torrent/42</comments>
   <link>https://jackett.local/dl/abc?path=x&amp;file=The.Matrix.torrent</link>
   <pubDate>Fri, 03 Jul 2026 10:11:12 +0000</pubDate>
   <size>8589934592</size>
   <enclosure url="https://jackett.local/dl/abc?path=x&amp;file=The.Matrix.torrent" length="8589934592" type="application/x-bittorrent" />
   <torznab:attr name="category" value="2000" />
   <torznab:attr name="seeders" value="12" />
   <torznab:attr name="peers" value="30" />
   <torznab:attr name="infohash" value="ABCDEF0123456789" />
   <torznab:attr name="magneturl" value="magnet:?xt=urn:btih:ABCDEF0123456789&amp;dn=The.Matrix" />
   <torznab:attr name="imdbid" value="tt0133093" />
   <torznab:attr name="tmdbid" value="603" />
  </item>
  <item>
   <title>Second Release 720p WEB x264</title>
   <guid>guid-2</guid>
   <link>magnet:?xt=urn:btih:FEED</link>
   <torznab:attr name="seeders" value="3" />
  </item>
 </channel>
</rss>"#;

    #[test]
    fn parses_jackett_rss_items() {
        let items = parse_items(RSS.as_bytes()).unwrap();
        assert_eq!(items.len(), 2);
        let r = &items[0];
        assert_eq!(r.title, "The.Matrix.1999.1080p.BluRay.x265-GRP");
        assert_eq!(r.guid, "https://tracker.example/details?id=42&page=1");
        assert_eq!(r.link.as_deref(), Some("https://jackett.local/dl/abc?path=x&file=The.Matrix.torrent"));
        assert_eq!(r.size_bytes, Some(8_589_934_592));
        assert_eq!(r.seeders, Some(12));
        assert_eq!(r.leechers, Some(18));
        assert_eq!(r.info_hash.as_deref(), Some("ABCDEF0123456789"));
        assert!(r.magnet.as_deref().unwrap().starts_with("magnet:?xt=urn:btih:ABCDEF0123456789&dn="));
        assert_eq!(r.imdb_id.as_deref(), Some("tt0133093"));
        assert_eq!(r.tmdb_id, Some(603));
        assert!(r.published_at.as_deref().unwrap().contains("2026"));
        // <comments> is the tracker's torrent page and wins over the guid URL.
        assert_eq!(r.details_url.as_deref(), Some("https://tracker.example/torrent/42"));

        // Magnet-only item: the magnet landed in `magnet`, not `link`.
        let r2 = &items[1];
        assert_eq!(r2.magnet.as_deref(), Some("magnet:?xt=urn:btih:FEED"));
        assert_eq!(r2.link, None);
        // No comments + non-URL guid: no details link.
        assert_eq!(r2.details_url, None);
    }

    #[test]
    fn surfaces_torznab_error_documents() {
        let xml = r#"<?xml version="1.0"?><error code="100" description="Incorrect user credentials" />"#;
        let err = parse_items(xml.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("100") && err.contains("credentials"), "{err}");
    }

    #[test]
    fn parses_caps_supported_params() {
        let xml = r#"<caps>
  <server title="Jackett" />
  <searching>
    <search available="yes" supportedParams="q" />
    <movie-search available="yes" supportedParams="q,imdbid,tmdbid" />
    <tv-search available="yes" supportedParams="q,season,ep" />
  </searching>
</caps>"#;
        let caps = parse_caps(xml.as_bytes()).unwrap();
        assert_eq!(caps.server_title.as_deref(), Some("Jackett"));
        assert!(caps.search_tmdb && caps.search_imdb);
        assert!(!caps.tv_search_tmdb);
    }
}
