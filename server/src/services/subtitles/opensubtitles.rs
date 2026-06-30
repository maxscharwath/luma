//! OpenSubtitles.com provider (REST API over `curl`, same transport approach as
//! the LLM/metadata clients). Search needs only the API key; downloading needs a
//! user login (token) and counts against the account's daily quota.
//!
//! Built behind the provider-agnostic [`super::RemoteSub`] shape so other sources
//! (Podnapisi, …) can be added later without touching callers.

use serde_json::Value;
use std::process::Command;

use super::RemoteSub;

const BASE: &str = "https://api.opensubtitles.com/api/v1";
const PROVIDER: &str = "opensubtitles";
/// OpenSubtitles requires a descriptive, versioned User-Agent.
const USER_AGENT: &str = "LUMA/1.0";

/// Search by title + year (no IMDb id needed), restricted to `langs` (e.g.
/// `["fr","en"]`). Returns the top results as provider-agnostic [`RemoteSub`]s.
pub fn search(api_key: &str, title: &str, year: Option<i64>, langs: &[String]) -> Vec<RemoteSub> {
    if api_key.is_empty() || title.is_empty() {
        return Vec::new();
    }
    let mut url = format!("{BASE}/subtitles?query={}&order_by=download_count&order_direction=desc", enc(title));
    if let Some(y) = year {
        url.push_str(&format!("&year={y}"));
    }
    if !langs.is_empty() {
        url.push_str(&format!("&languages={}", langs.join(",")));
    }
    let Some(json) = curl_get_json(&url, api_key, None) else {
        return Vec::new();
    };
    let Some(data) = json.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|d| parse_result(d))
        .filter(|s| langs.is_empty() || langs.iter().any(|l| l.eq_ignore_ascii_case(&s.language)))
        .take(40)
        .collect()
}

/// One OpenSubtitles search hit → a [`RemoteSub`]. `id` is the `file_id` we later
/// download (the API requires the file id, not the subtitle id).
fn parse_result(d: &Value) -> Option<RemoteSub> {
    let attrs = d.get("attributes")?;
    let language = attrs.get("language").and_then(Value::as_str).unwrap_or("").to_lowercase();
    let files = attrs.get("files").and_then(Value::as_array)?;
    let file = files.first()?;
    let file_id = file.get("file_id").and_then(Value::as_i64)?;
    let release = attrs.get("release").and_then(Value::as_str).unwrap_or("");
    let downloads = attrs.get("download_count").and_then(Value::as_u64).unwrap_or(0) as u32;
    let hi = attrs.get("hearing_impaired").and_then(Value::as_bool).unwrap_or(false);
    let label = if release.is_empty() {
        format!("{} {}", language.to_uppercase(), if hi { "(SDH)" } else { "" }).trim().to_string()
    } else {
        format!("{release}{}", if hi { " (SDH)" } else { "" })
    };
    Some(RemoteSub { id: file_id.to_string(), provider: PROVIDER.to_string(), language, label, downloads })
}

/// Download the subtitle file (`file_id` = [`RemoteSub::id`]) and return its raw
/// text (SRT/VTT). Requires a user login; the token is fetched per call (cheap and
/// avoids stale-token handling for the low call volume here).
pub fn download(api_key: &str, username: &str, password: &str, file_id: &str) -> Option<String> {
    if api_key.is_empty() || username.is_empty() || password.is_empty() {
        return None;
    }
    let token = login(api_key, username, password)?;
    let body = serde_json::json!({ "file_id": file_id.parse::<i64>().ok()? });
    let resp = curl_post_json(&format!("{BASE}/download"), api_key, Some(&token), &body)?;
    let link = resp.get("link").and_then(Value::as_str)?;
    curl_get_text(link)
}

/// Exchange username/password for a bearer token (valid ~24h; we don't cache it).
fn login(api_key: &str, username: &str, password: &str) -> Option<String> {
    let body = serde_json::json!({ "username": username, "password": password });
    let resp = curl_post_json(&format!("{BASE}/login"), api_key, None, &body)?;
    resp.get("token").and_then(Value::as_str).map(str::to_string)
}

// ----- curl transport ---------------------------------------------------------

fn base_headers<'a>(cmd: &mut Command, api_key: &str, bearer: Option<&str>) {
    cmd.arg("-H").arg(format!("Api-Key: {api_key}"));
    cmd.arg("-H").arg(format!("User-Agent: {USER_AGENT}"));
    cmd.arg("-H").arg("Accept: application/json");
    if let Some(t) = bearer {
        cmd.arg("-H").arg(format!("Authorization: Bearer {t}"));
    }
}

fn curl_get_json(url: &str, api_key: &str, bearer: Option<&str>) -> Option<Value> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-S", "--max-time", "20"]);
    base_headers(&mut cmd, api_key, bearer);
    cmd.arg(url);
    run_json(cmd)
}

fn curl_post_json(url: &str, api_key: &str, bearer: Option<&str>, body: &Value) -> Option<Value> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-S", "--max-time", "20", "-X", "POST"]);
    base_headers(&mut cmd, api_key, bearer);
    cmd.arg("-H").arg("Content-Type: application/json");
    cmd.arg("-d").arg(body.to_string());
    cmd.arg(url);
    run_json(cmd)
}

fn run_json(mut cmd: Command) -> Option<Value> {
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// GET a (possibly large, non-JSON) body as UTF-8 text - the subtitle download link.
fn curl_get_text(url: &str) -> Option<String> {
    let out = Command::new("curl").args(["-sSL", "--max-time", "30", url]).output().ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Minimal percent-encoding for a query-string value (RFC 3986 unreserved kept).
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
