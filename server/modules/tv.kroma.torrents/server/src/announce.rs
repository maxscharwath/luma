//! Self-managed tracker announce that feeds the embedded engine peers it can't
//! get for itself behind the VPN. librqbit dials trackers through reqwest, whose
//! SOCKS support can't traverse the WireGuard-to-SOCKS bridge (it fails
//! "host unreachable"), so with a proxy configured the engine's OWN tracker
//! announce yields nothing - fatal for a private, IPv6-only swarm (no DHT
//! fallback). We announce ourselves over the bridge with `curl`
//! (`--socks5-hostname`, which DOES traverse it), parse BOTH `peers` and
//! `peers6`, and hand librqbit the result as `initial_peers` - which it can then
//! connect to (over IPv6 too). Used both as the one-shot seed at add time and by
//! the periodic re-seed of stalled torrents (see `DownloadManager::reseed_stalled`).
//!
//! Best-effort throughout: any parse/network failure yields an empty list and
//! the engine falls back to its own discovery.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use sha1::{Digest, Sha1};

/// Announce to every HTTP tracker in the `.torrent` and return the union of
/// peers (IPv4 + IPv6), deduped.
pub fn tracker_peers(torrent_bytes: &[u8], socks: Option<&str>) -> Vec<SocketAddr> {
    let Some(info_hash) = info_hash(torrent_bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for url in announce_urls(torrent_bytes).into_iter().take(4) {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue; // UDP trackers need a different protocol; skip.
        }
        for addr in announce_once(&url, &info_hash, socks) {
            if seen.insert(addr) {
                out.push(addr);
            }
        }
    }
    out
}

/// One HTTP announce; returns the peers it reported (both families).
fn announce_once(base: &str, info_hash: &[u8; 20], socks: Option<&str>) -> Vec<SocketAddr> {
    // A fixed-but-torrent-specific peer id (`-LM0001-` + hash nibbles).
    let mut peer_id = *b"-LM0001-000000000000";
    for (i, b) in info_hash.iter().take(6).enumerate() {
        let hex = b"0123456789abcdef";
        peer_id[8 + i * 2] = hex[(b >> 4) as usize];
        peer_id[9 + i * 2] = hex[(b & 0xf) as usize];
    }
    let q = format!(
        "info_hash={}&peer_id={}&port=6881&uploaded=0&downloaded=0&left=1&numwant=200&compact=1&supportcrypto=1&event=started",
        urlencode(info_hash),
        urlencode(&peer_id),
    );
    let sep = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{sep}{q}");

    let mut fetch = kroma_module_sdk::http::Fetch::new().max_time(20);
    if let Some(proxy) = socks {
        fetch = fetch.socks5(proxy.to_string());
    }
    match fetch.get(&url).and_then(|r| r.ensure_ok()) {
        Ok(resp) => parse_peers(&resp.body),
        Err(_) => Vec::new(),
    }
}

/// Percent-encode arbitrary bytes (BitTorrent info_hash / peer_id style).
fn urlencode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            s.push(b as char);
        } else {
            s.push('%');
            s.push_str(&format!("{b:02X}"));
        }
    }
    s
}

/// Extract `peers` (6-byte v4) + `peers6` (18-byte v6) from a compact tracker
/// response (a flat bencoded dict).
fn parse_peers(body: &[u8]) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for (key, val) in top_dict_strings(body) {
        match key {
            b"peers" => {
                for c in val.chunks_exact(6) {
                    let ip = Ipv4Addr::new(c[0], c[1], c[2], c[3]);
                    let port = u16::from_be_bytes([c[4], c[5]]);
                    out.push(SocketAddr::V4(SocketAddrV4::new(ip, port)));
                }
            }
            b"peers6" => {
                for c in val.chunks_exact(18) {
                    let mut ip = [0u8; 16];
                    ip.copy_from_slice(&c[..16]);
                    let port = u16::from_be_bytes([c[16], c[17]]);
                    out.push(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(ip), port, 0, 0)));
                }
            }
            _ => {}
        }
    }
    out
}

// ----- minimal bencode --------------------------------------------------------

/// End index (exclusive) of the bencode value starting at `i`.
fn bskip(b: &[u8], i: usize) -> Option<usize> {
    match b.get(i)? {
        b'i' => b[i..].iter().position(|&c| c == b'e').map(|p| i + p + 1),
        b'l' => {
            let mut j = i + 1;
            while *b.get(j)? != b'e' {
                j = bskip(b, j)?;
            }
            Some(j + 1)
        }
        b'd' => {
            let mut j = i + 1;
            while *b.get(j)? != b'e' {
                j = bskip(b, j)?; // key
                j = bskip(b, j)?; // value
            }
            Some(j + 1)
        }
        b'0'..=b'9' => {
            let colon = i + b[i..].iter().position(|&c| c == b':')?;
            let len: usize = std::str::from_utf8(b.get(i..colon)?).ok()?.parse().ok()?;
            Some(colon + 1 + len)
        }
        _ => None,
    }
}

/// Read a bencode byte string at `i`, returning `(bytes, next_index)`.
fn read_bstr(b: &[u8], i: usize) -> Option<(&[u8], usize)> {
    let colon = i + b.get(i..)?.iter().position(|&c| c == b':')?;
    let len: usize = std::str::from_utf8(b.get(i..colon)?).ok()?.parse().ok()?;
    let start = colon + 1;
    Some((b.get(start..start + len)?, start + len))
}

/// SHA-1 of the `.torrent`'s `info` dict = the info hash.
fn info_hash(b: &[u8]) -> Option<[u8; 20]> {
    let (start, end) = dict_value_bounds(b, b"info")?;
    let mut h = Sha1::new();
    h.update(b.get(start..end)?);
    Some(h.finalize().into())
}

/// The `announce` string + every `announce-list` entry.
fn announce_urls(b: &[u8]) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some((s, e)) = dict_value_bounds(b, b"announce") {
        if let Some((bytes, _)) = read_bstr(b, s) {
            if e >= s {
                urls.push(String::from_utf8_lossy(bytes).into_owned());
            }
        }
    }
    // announce-list = list of lists of strings.
    if let Some((s, e)) = dict_value_bounds(b, b"announce-list") {
        let mut i = s + 1; // into the outer list
        while i < e && b.get(i) == Some(&b'l') {
            let mut j = i + 1; // into an inner list
            while j < e && b.get(j) != Some(&b'e') {
                if let Some((bytes, nj)) = read_bstr(b, j) {
                    urls.push(String::from_utf8_lossy(bytes).into_owned());
                    j = nj;
                } else {
                    break;
                }
            }
            i = j + 1;
        }
    }
    urls
}

/// Byte-range of the value for `key` in the TOP-LEVEL dict.
fn dict_value_bounds(b: &[u8], key: &[u8]) -> Option<(usize, usize)> {
    if b.first()? != &b'd' {
        return None;
    }
    let mut i = 1;
    while *b.get(i)? != b'e' {
        let (k, after_key) = read_bstr(b, i)?;
        let vend = bskip(b, after_key)?;
        if k == key {
            return Some((after_key, vend));
        }
        i = vend;
    }
    None
}

/// Top-level dict keys whose values are byte strings, as `(key, value)`.
fn top_dict_strings(b: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    if b.first() != Some(&b'd') {
        return out;
    }
    let mut i = 1;
    while let Some(&c) = b.get(i) {
        if c == b'e' {
            break;
        }
        let Some((k, after_key)) = read_bstr(b, i) else { break };
        let Some(vend) = bskip(b, after_key) else { break };
        if b.get(after_key) == Some(&b'0')
            || matches!(b.get(after_key), Some(b'1'..=b'9'))
        {
            if let Some((val, _)) = read_bstr(b, after_key) {
                out.push((k, val));
            }
        }
        i = vend;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tiny synthetic tracker response with one v4 + one v6 peer.
    #[test]
    fn parses_v4_and_v6_peers() {
        let mut v = Vec::new();
        v.extend_from_slice(b"d8:completei5e");
        // peers = 1.2.3.4:6881
        v.extend_from_slice(b"5:peers6:");
        v.extend_from_slice(&[1, 2, 3, 4, 0x1a, 0xe1]);
        // peers6 = [::1]:6882
        v.extend_from_slice(b"6:peers618:");
        let mut ip6 = [0u8; 16];
        ip6[15] = 1;
        v.extend_from_slice(&ip6);
        v.extend_from_slice(&[0x1a, 0xe2]);
        v.push(b'e');
        let peers = parse_peers(&v);
        assert_eq!(peers.len(), 2);
        assert!(peers.iter().any(|p| p.is_ipv4() && p.port() == 6881));
        assert!(peers.iter().any(|p| p.is_ipv6() && p.port() == 6882));
    }

    #[test]
    fn computes_info_hash_and_announce() {
        // Outer dict { announce: "url", info: { foo: "bar" } }.
        let t = b"d8:announce3:url4:infod3:foo3:baree";
        let ih = info_hash(t).expect("hash");
        // The info hash is SHA-1 of the info dict bytes only: "d3:foo3:bare".
        let mut h = Sha1::new();
        h.update(b"d3:foo3:bare");
        let expect: [u8; 20] = h.finalize().into();
        assert_eq!(ih, expect);
        assert_eq!(announce_urls(t), vec!["url".to_string()]);
    }
}
