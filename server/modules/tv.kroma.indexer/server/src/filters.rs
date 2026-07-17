//! The Cardigann field/keyword filter pipeline.
//!
//! A filter takes the current string value (plus zero or more arguments) and
//! returns a transformed string. Filters chain in declaration order. Arguments
//! may themselves be templates (`append "{{ if .Config.x }}…{{ end }}"`), so
//! they are rendered against the context before the filter runs.
//!
//! The commonly-used filters are implemented faithfully; unknown filters pass
//! the value through unchanged (a definition using one exotic filter still
//! returns usable releases) - see [`apply_one`].

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};

use crate::context::Context;
use crate::definition::Filter;
use crate::template;

/// Run a filter chain over `value`.
pub fn apply(value: &str, filters: &[Filter], ctx: &Context) -> String {
    let mut v = value.to_string();
    for f in filters {
        // Filter args can be templated.
        let args: Vec<String> = f.args.iter().map(|a| template::render(a, ctx)).collect();
        v = apply_one(&f.name, &v, &args);
    }
    v
}

fn apply_one(name: &str, value: &str, args: &[String]) -> String {
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match name {
        "re_replace" => match regex::Regex::new(arg(0)) {
            Ok(re) => re.replace_all(value, arg(1)).into_owned(),
            Err(_) => value.to_string(),
        },
        "replace" => value.replace(arg(0), arg(1)),
        "trim" => {
            if args.is_empty() {
                value.trim().to_string()
            } else {
                let cut: Vec<char> = arg(0).chars().collect();
                value.trim_matches(|c| cut.contains(&c)).to_string()
            }
        }
        "trimprefix" => value.strip_prefix(arg(0)).unwrap_or(value).to_string(),
        "trimsuffix" => value.strip_suffix(arg(0)).unwrap_or(value).to_string(),
        "prepend" => format!("{}{}", arg(0), value),
        "append" => format!("{}{}", value, arg(0)),
        "tolower" => value.to_lowercase(),
        "toupper" => value.to_uppercase(),
        "split" => split(value, arg(0), arg(1)),
        "regexp" => regexp_extract(value, arg(0)),
        "htmldecode" => html_decode(value),
        "htmlencode" => html_encode(value),
        "urldecode" => url_decode(value),
        "urlencode" => url_encode(value),
        "validate" => validate(value, arg(0)),
        "validfilename" => valid_filename(value),
        "diacritics" => remove_diacritics(value),
        "querystring" => query_param(value, arg(0)),
        "dateparse" | "date" => date_parse(value, args),
        "timeago" | "reltime" | "fuzzytime" | "timeparse" => reltime(value),
        // Debug helpers + not-yet-modeled filters: pass through unchanged.
        "hexdump" | "strdump" | "widthfix" => value.to_string(),
        _ => value.to_string(),
    }
}

// ----- string filters -------------------------------------------------------------

/// Split on a separator and take the element at `index` (negative counts from
/// the end, Cardigann-style). Out-of-range yields the original value.
fn split(value: &str, sep: &str, index: &str) -> String {
    if sep.is_empty() {
        return value.to_string();
    }
    let parts: Vec<&str> = value.split(sep).collect();
    let idx: i64 = index.parse().unwrap_or(0);
    let i = if idx < 0 { parts.len() as i64 + idx } else { idx };
    if i >= 0 && (i as usize) < parts.len() {
        parts[i as usize].to_string()
    } else {
        value.to_string()
    }
}

/// Return the first capture group of `pattern` (or the whole match if the
/// pattern has no groups), else empty.
fn regexp_extract(value: &str, pattern: &str) -> String {
    match regex::Regex::new(pattern) {
        Ok(re) => match re.captures(value) {
            Some(caps) => caps
                .get(1)
                .or_else(|| caps.get(0))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
            None => String::new(),
        },
        Err(_) => value.to_string(),
    }
}

/// Keep the value only if it is one of the comma-separated allowed values
/// (case-insensitive), else empty. Used to whitelist e.g. genres.
fn validate(value: &str, allowed: &str) -> String {
    let ok = allowed
        .split(',')
        .map(str::trim)
        .any(|a| a.eq_ignore_ascii_case(value.trim()));
    if ok {
        value.to_string()
    } else {
        String::new()
    }
}

fn valid_filename(value: &str) -> String {
    value.replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "")
}

/// Strip common Latin diacritics (à->a, é->e, ü->u, ñ->n…). A pragmatic table,
/// not full Unicode normalization.
fn remove_diacritics(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'È' | 'É' | 'Ê' | 'Ë' => 'E',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'O',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

fn query_param(value: &str, key: &str) -> String {
    let query = value.split_once('?').map(|(_, q)| q).unwrap_or(value);
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return url_decode(v);
            }
        }
    }
    String::new()
}

// ----- url / html en-/decoding ----------------------------------------------------

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode (RFC 3986 unreserved). `pub(crate)` so the session layer's
/// form/query encoding shares one implementation.
pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn html_decode(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        if let Some(semi) = rest.find(';').filter(|&i| i <= 10) {
            let entity = &rest[1..semi];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{00a0}'),
                e if e.starts_with("#x") || e.starts_with("#X") => {
                    u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)
                }
                e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            match decoded {
                Some(c) => {
                    out.push(c);
                    rest = &rest[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn html_encode(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ----- date handling --------------------------------------------------------------

/// Normalized output for a parsed date: RFC 3339 (the domain layer only needs
/// *a* parseable timestamp for age display).
fn to_rfc3339(dt: NaiveDateTime) -> String {
    Utc.from_utc_datetime(&dt).to_rfc3339()
}

/// `dateparse`/`date`: try each supplied Go layout, then a set of common
/// formats, then relative parsing.
fn date_parse(value: &str, layouts: &[String]) -> String {
    let v = value.trim();
    for layout in layouts {
        let fmt = go_layout_to_chrono(layout);
        if let Ok(dt) = NaiveDateTime::parse_from_str(v, &fmt) {
            return to_rfc3339(dt);
        }
        if let Ok(d) = NaiveDate::parse_from_str(v, &fmt) {
            return to_rfc3339(d.and_hms_opt(0, 0, 0).unwrap());
        }
    }
    parse_fuzzy(v).unwrap_or_else(|| value.to_string())
}

/// `timeago`/`reltime`/`fuzzytime`: relative ("3 hours ago", "yesterday") and
/// common absolute formats.
fn reltime(value: &str) -> String {
    parse_fuzzy(value.trim()).unwrap_or_else(|| value.to_string())
}

fn parse_fuzzy(v: &str) -> Option<String> {
    if let Some(dt) = parse_relative(v) {
        return Some(to_rfc3339(dt));
    }
    // RFC 2822 (`Tue, 30 Jun 2026 11:19:47 +0000`, common in RSS/Torznab
    // `pubDate`) and RFC 3339, both carrying a timezone offset.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(v) {
        return Some(dt.to_utc().to_rfc3339());
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(v) {
        return Some(dt.to_utc().to_rfc3339());
    }
    const FORMATS: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%d-%m-%Y %H:%M:%S",
        "%d-%m-%Y %H:%M",
        "%d/%m/%Y %H:%M:%S",
        "%d/%m/%Y %H:%M",
        "%d/%m/%Y",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y",
        "%d.%m.%Y %H:%M:%S",
        "%d.%m.%Y",
        "%b %d %Y, %H:%M",
        "%b %d, %Y",
        "%d %b %Y %H:%M:%S",
        "%d %b %Y",
        "%B %d, %Y",
    ];
    for fmt in FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(v, fmt) {
            return Some(to_rfc3339(dt));
        }
        if let Ok(d) = NaiveDate::parse_from_str(v, fmt) {
            return Some(to_rfc3339(d.and_hms_opt(0, 0, 0).unwrap()));
        }
    }
    None
}

/// Parse relative expressions against the local clock: "just now",
/// "5 minutes ago", "2 hours ago", "yesterday", "today", "3 days ago",
/// "1 week ago", "2 months ago", "1 year ago".
fn parse_relative(v: &str) -> Option<NaiveDateTime> {
    let now = Local::now().naive_local();
    let lower = v.to_lowercase();
    let lower = lower.trim();
    if lower == "just now" || lower == "now" {
        return Some(now);
    }
    if lower == "today" {
        return now.date().and_hms_opt(0, 0, 0);
    }
    if lower == "yesterday" {
        return (now - Duration::days(1)).date().and_hms_opt(0, 0, 0);
    }
    // "<n> <unit> ago" (also "a"/"an" for 1).
    let re = regex::Regex::new(
        r"(?i)(?:(\d+)|a|an)\s*(sec|second|min|minute|hour|day|week|month|year)s?\s*(?:ago)?",
    )
    .ok()?;
    let caps = re.captures(lower)?;
    let n: i64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(1);
    let unit = caps.get(2)?.as_str();
    let dt = match unit {
        "sec" | "second" => now - Duration::seconds(n),
        "min" | "minute" => now - Duration::minutes(n),
        "hour" => now - Duration::hours(n),
        "day" => now - Duration::days(n),
        "week" => now - Duration::weeks(n),
        "month" => sub_months(now, n),
        "year" => now.with_year(now.year() - n as i32).unwrap_or(now),
        _ => return None,
    };
    Some(dt)
}

fn sub_months(dt: NaiveDateTime, months: i64) -> NaiveDateTime {
    let total = dt.year() as i64 * 12 + (dt.month0() as i64) - months;
    let year = (total.div_euclid(12)) as i32;
    let month0 = total.rem_euclid(12) as u32;
    let day = dt.day().min(28); // clamp to avoid invalid dates
    NaiveDate::from_ymd_opt(year, month0 + 1, day)
        .and_then(|d| d.and_hms_opt(dt.hour(), dt.minute(), dt.second()))
        .unwrap_or(dt)
}

/// Translate a Go reference-time layout ("2006-01-02 15:04:05") into a chrono
/// `strftime` format. Longest tokens first so `2006` isn't split.
fn go_layout_to_chrono(layout: &str) -> String {
    const SUBS: &[(&str, &str)] = &[
        ("2006", "%Y"),
        ("January", "%B"),
        ("Monday", "%A"),
        ("-07:00", "%:z"),
        ("-0700", "%z"),
        ("15", "%H"),
        ("Jan", "%b"),
        ("Mon", "%a"),
        ("MST", "%Z"),
        (".000", "%.3f"),
        ("06", "%y"),
        ("01", "%m"),
        ("02", "%d"),
        ("_2", "%e"),
        ("03", "%I"),
        ("04", "%M"),
        ("05", "%S"),
        ("PM", "%p"),
    ];
    let mut out = String::new();
    let bytes = layout.as_bytes();
    let mut i = 0;
    'outer: while i < bytes.len() {
        for (go, cr) in SUBS {
            if layout[i..].starts_with(go) {
                out.push_str(cr);
                i += go.len();
                continue 'outer;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::Filter;

    fn f(name: &str, args: &[&str]) -> Filter {
        Filter { name: name.into(), args: args.iter().map(|s| s.to_string()).collect() }
    }

    fn run(value: &str, filters: &[Filter]) -> String {
        apply(value, filters, &Context::default())
    }

    #[test]
    fn basic_string_filters() {
        assert_eq!(run("Hello", &[f("tolower", &[])]), "hello");
        assert_eq!(run(" x ", &[f("trim", &[])]), "x");
        assert_eq!(run("a-b-c", &[f("replace", &["-", " "])]), "a b c");
        assert_eq!(run("cat=42&x=1", &[f("regexp", &["cat=(\\d+)"])]), "42");
        assert_eq!(run("/a/b/c", &[f("split", &["/", "2"])]), "b");
        assert_eq!(run("/a/b/c", &[f("split", &["/", "-1"])]), "c");
        assert_eq!(run("abc", &[f("prepend", &["/dl/"]), f("append", &[".torrent"])]), "/dl/abc.torrent");
    }

    #[test]
    fn regexp_replace_chain() {
        let out = run("The.Matrix.1999", &[f("re_replace", &["\\.", " "])]);
        assert_eq!(out, "The Matrix 1999");
    }

    #[test]
    fn url_and_html() {
        assert_eq!(run("a%20b%2Fc", &[f("urldecode", &[])]), "a b/c");
        assert_eq!(run("a b/c", &[f("urlencode", &[])]), "a%20b%2Fc");
        assert_eq!(run("Tom &amp; Jerry &#39;s", &[f("htmldecode", &[])]), "Tom & Jerry 's");
    }

    #[test]
    fn validate_and_filename() {
        assert_eq!(run("Action", &[f("validate", &["Action, Drama"])]), "Action");
        assert_eq!(run("Nope", &[f("validate", &["Action, Drama"])]), "");
        assert_eq!(run("a/b:c", &[f("validfilename", &[])]), "abc");
    }

    #[test]
    fn querystring_filter() {
        assert_eq!(run("https://x/dl?id=99&k=v", &[f("querystring", &["id"])]), "99");
    }

    #[test]
    fn date_absolute_and_layout() {
        // ISO absolute via the fuzzy format table.
        let out = run("2023-05-01 12:30:00", &[f("timeago", &[])]);
        assert!(out.starts_with("2023-05-01T12:30:00"), "got {out}");
        // Explicit Go layout.
        let out = run("01/05/2023", &[f("dateparse", &["02/01/2006"])]);
        assert!(out.starts_with("2023-05-01"), "got {out}");
    }

    #[test]
    fn date_relative_is_parseable() {
        // Relative dates resolve against "now"; just assert we produced an
        // RFC-3339-looking timestamp rather than the literal input.
        let out = run("3 days ago", &[f("timeago", &[])]);
        assert!(out.contains('T') && out.len() >= 19, "got {out}");
        let out = run("just now", &[f("reltime", &[])]);
        assert!(out.contains('T'), "got {out}");
    }

    #[test]
    fn diacritics_filter() {
        assert_eq!(run("Amélie Café", &[f("diacritics", &[])]), "Amelie Cafe");
    }
}
