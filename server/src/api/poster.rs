//! Deterministic SVG placeholder posters. No real artwork in the scaffold a
//! 2:3 two-stop gradient derived from the item id, with the title near the
//! bottom.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// Render an inline SVG poster for an item.
pub fn render_poster(id: &str, title: &str) -> Response {
    let svg = build_svg(id, title);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        svg,
    )
        .into_response()
}

/// Build the SVG markup. Colours are derived deterministically from the id so a
/// given item always gets the same poster.
fn build_svg(id: &str, title: &str) -> String {
    let (hue_a, hue_b) = hues_from_id(id);
    let color_a = format!("hsl({}, 62%, 38%)", hue_a);
    let color_b = format!("hsl({}, 58%, 18%)", hue_b);

    let safe_title = xml_escape(title);
    let initials = xml_escape(&initials_of(title));
    let grad_id = format!("g{}", &id.chars().take(8).collect::<String>());

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="600" viewBox="0 0 400 600" role="img" aria-label="{title}">
  <defs>
    <linearGradient id="{grad}" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="{a}"/>
      <stop offset="100%" stop-color="{b}"/>
    </linearGradient>
    <linearGradient id="{grad}-scrim" x1="0" y1="0" x2="0" y2="1">
      <stop offset="55%" stop-color="rgba(0,0,0,0)"/>
      <stop offset="100%" stop-color="rgba(0,0,0,0.65)"/>
    </linearGradient>
  </defs>
  <rect width="400" height="600" fill="url(#{grad})"/>
  <text x="200" y="300" text-anchor="middle" dominant-baseline="middle"
        font-family="Helvetica, Arial, sans-serif" font-size="120" font-weight="700"
        fill="rgba(255,255,255,0.16)">{initials}</text>
  <rect width="400" height="600" fill="url(#{grad}-scrim)"/>
  <text x="28" y="556" text-anchor="start"
        font-family="Helvetica, Arial, sans-serif" font-size="26" font-weight="600"
        fill="#ffffff">{title}</text>
</svg>"##,
        title = safe_title,
        initials = initials,
        grad = grad_id,
        a = color_a,
        b = color_b,
    )
}

/// Derive two distinct hues (0–359) from the id's hex characters.
fn hues_from_id(id: &str) -> (u32, u32) {
    let sum: u32 = id.bytes().map(|b| b as u32).sum();
    let hue_a = sum % 360;
    let hue_b = (hue_a + 40) % 360;
    (hue_a, hue_b)
}

/// First letters of up to two words, uppercased.
fn initials_of(title: &str) -> String {
    let letters: String = title
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect();
    if letters.is_empty() {
        "?".to_string()
    } else {
        letters.to_uppercase()
    }
}

/// Minimal XML escaping for text nodes / attributes.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
