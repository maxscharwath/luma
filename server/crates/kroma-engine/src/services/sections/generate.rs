//! LLM authorship of personalized home sections: turn a user's taste [`Cluster`]s
//! into a small set of catchy, localized section names + an English "vibe" query
//! the embedder resolves to real catalog items. Also the evolving natural-language
//! taste profile. Prompt building + response parsing live here (pure + tested);
//! the orchestration (iterate users, call the model, persist) is the
//! `sections.personalize` job in [`crate::services::jobs`].

use serde::{Deserialize, Serialize};

use crate::db::{self, Pool};

use super::taste::Cluster;

/// How many sections we ask the model for (and cap to).
const MAX_SECTIONS: usize = 6;
/// Cap on a single section title, defended on parse (catchy, not an essay).
const MAX_TITLE: usize = 48;

/// A personalized section authored by the LLM and cached per user. `query` is the
/// embedding search phrase (English vibe); `title`/`reason` are in the user's
/// locale. Stored as a JSON array in `user_taste.sections`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenSection {
    pub key: String,
    pub title: String,
    pub query: String,
    #[serde(default)]
    pub reason: String,
}

/// Load a user's cached personalized sections (empty if none / malformed).
pub fn load(pool: &Pool, user_id: &str) -> Vec<GenSection> {
    let Ok(Some((_, json))) = db::get_user_taste(pool, user_id) else {
        return Vec::new();
    };
    serde_json::from_str(&json).unwrap_or_default()
}

/// Build the (system, user) prompt for one user's clusters. `locale` is the
/// account's UI language code (`"fr"`/`"en"`); `prev_profile` is last run's
/// profile, if any, so the model refines rather than restarts.
pub fn build_prompt(locale: &str, prev_profile: Option<&str>, clusters: &[Cluster]) -> (String, String) {
    let lang = language_name(locale);
    let system = format!(
        "You are the personalization curator for a home-media library. From a viewer's \
         taste groups you write a short taste profile and name a few personalized rows for \
         their home screen.\n\
         Reply with STRICT JSON only no prose, no markdown, no code fences shaped exactly:\n\
         {{\"profile\": string, \"sections\": [{{\"title\": string, \"query\": string, \"reason\": string}}]}}\n\
         Rules:\n\
         - Write \"profile\" (2-3 sentences) and every \"title\" and \"reason\" in {lang}.\n\
         - \"title\": a catchy row name under 6 words. \"reason\": one short clause ('because you …').\n\
         - \"query\": an ENGLISH phrase (5-12 words) describing the vibe/genre/mood, used to \
         search the library by meaning. Do NOT put specific movie titles in \"query\".\n\
         - Give between 3 and {MAX_SECTIONS} distinct sections covering the groups below."
    );

    let mut user = String::new();
    if let Some(p) = prev_profile.filter(|p| !p.trim().is_empty()) {
        user.push_str(&format!("Previous taste profile (refine it): {p}\n\n"));
    }
    user.push_str("Taste groups (from what they've watched):\n");
    for (i, c) in clusters.iter().enumerate() {
        user.push_str(&format!(
            "Group {}: examples = [{}]; genres = [{}]; keywords = [{}]\n",
            i + 1,
            c.titles.join(", "),
            c.genres.join(", "),
            c.keywords.join(", "),
        ));
    }
    user.push_str("\nReturn the JSON now.");
    (system, user)
}

#[derive(Deserialize)]
struct LlmOut {
    #[serde(default)]
    profile: String,
    #[serde(default)]
    sections: Vec<LlmSection>,
}

#[derive(Deserialize)]
struct LlmSection {
    #[serde(default)]
    title: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    reason: String,
}

/// Parse a model reply into `(profile, sections)`. Tolerant of code fences and
/// surrounding prose: extracts the outermost `{…}` and validates each section.
pub fn parse_response(text: &str) -> anyhow::Result<(String, Vec<GenSection>)> {
    let json = extract_json(text).ok_or_else(|| anyhow::anyhow!("no JSON object in reply"))?;
    let out: LlmOut = serde_json::from_str(json)?;

    let mut sections = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in out.sections {
        let title = s.title.trim();
        let query = s.query.trim();
        if title.is_empty() || query.is_empty() {
            continue;
        }
        let title: String = title.chars().take(MAX_TITLE).collect();
        let mut key = slug(&title);
        if key.is_empty() {
            key = format!("s{}", sections.len() + 1);
        }
        if !seen.insert(key.clone()) {
            continue; // drop duplicate rows
        }
        sections.push(GenSection {
            key,
            title,
            query: query.to_string(),
            reason: s.reason.trim().to_string(),
        });
        if sections.len() >= MAX_SECTIONS {
            break;
        }
    }
    Ok((out.profile.trim().to_string(), sections))
}

/// Find the outermost JSON object in `text` (handles ```json fences / preamble).
fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(&text[start..=end])
    } else {
        None
    }
}

/// ASCII slug for a section key (`"Neon Noir Nights"` → `"neon-noir-nights"`).
pub(crate) fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn language_name(locale: &str) -> &'static str {
    match locale {
        "en" => "English",
        _ => "French",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let reply = r#"{"profile":"You love stylish crime.","sections":[
            {"title":"Neon Noir Nights","query":"neon-soaked night crime thriller","reason":"because you love stylish crime"},
            {"title":"Mind Benders","query":"surreal mind-bending science fiction","reason":"you enjoy puzzles"}
        ]}"#;
        let (profile, sections) = parse_response(reply).unwrap();
        assert_eq!(profile, "You love stylish crime.");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].key, "neon-noir-nights");
        assert_eq!(sections[1].query, "surreal mind-bending science fiction");
    }

    #[test]
    fn tolerates_code_fences_and_prose() {
        let reply = "Sure! Here you go:\n```json\n{\"profile\":\"p\",\"sections\":[{\"title\":\"Cozy Classics\",\"query\":\"warm cozy classic comfort films\",\"reason\":\"r\"}]}\n```\nEnjoy!";
        let (_, sections) = parse_response(reply).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "Cozy Classics");
    }

    #[test]
    fn drops_invalid_and_duplicate_sections() {
        let reply = r#"{"sections":[
            {"title":"","query":"q"},
            {"title":"Action","query":""},
            {"title":"Action Fix","query":"high octane action"},
            {"title":"Action Fix","query":"another action"}
        ]}"#;
        let (_, sections) = parse_response(reply).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].key, "action-fix");
    }

    #[test]
    fn prompt_carries_groups_and_language() {
        let clusters = vec![Cluster {
            ids: vec!["1".into()],
            titles: vec!["Blade Runner".into()],
            genres: vec!["Science Fiction".into()],
            keywords: vec!["dystopia".into()],
        }];
        let (system, user) = build_prompt("en", Some("prev"), &clusters);
        assert!(system.contains("English"));
        assert!(user.contains("Blade Runner"));
        assert!(user.contains("Previous taste profile"));
    }

    #[test]
    fn build_prompt_omits_blank_previous_profile_and_uses_french() {
        let clusters = vec![Cluster { ids: vec![], titles: vec![], genres: vec![], keywords: vec![] }];
        let (system, user) = build_prompt("fr", Some("   "), &clusters);
        assert!(system.contains("French"));
        assert!(!user.contains("Previous taste profile")); // blank prev skipped
        let (_s, user2) = build_prompt("de", None, &clusters);
        assert!(!user2.contains("Previous taste profile"));
    }

    #[test]
    fn slug_handles_edges() {
        assert_eq!(slug("Neon Noir Nights"), "neon-noir-nights");
        assert_eq!(slug("  --Hello, World!! --"), "hello-world");
        assert_eq!(slug("café déjà"), "caf-d-j"); // non-ascii chars act as separators
        assert_eq!(slug("!!!"), "");
        assert_eq!(slug(""), "");
    }

    #[test]
    fn extract_json_finds_object_or_none() {
        assert_eq!(extract_json("prefix {\"a\":1} suffix"), Some("{\"a\":1}"));
        assert!(extract_json("no braces").is_none());
        assert!(extract_json("}before{").is_none()); // end <= start
    }

    #[test]
    fn language_name_maps_en_else_french() {
        assert_eq!(language_name("en"), "English");
        assert_eq!(language_name("fr"), "French");
        assert_eq!(language_name("xx"), "French");
    }

    #[test]
    fn parse_response_caps_at_max_sections() {
        let mut secs = String::new();
        for i in 0..10 {
            secs.push_str(&format!("{{\"title\":\"Row {i}\",\"query\":\"vibe {i} words here\"}},"));
        }
        let reply = format!("{{\"sections\":[{}]}}", secs.trim_end_matches(','));
        let (_, sections) = parse_response(&reply).unwrap();
        assert_eq!(sections.len(), MAX_SECTIONS);
    }

    #[test]
    fn parse_response_errors_without_json() {
        assert!(parse_response("no json here").is_err());
    }

    #[test]
    fn load_returns_empty_when_no_taste_row() {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kroma-gen-load-{}-{n}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pool = crate::db::init(&path).unwrap();
        assert!(load(&pool, "nobody").is_empty());
    }
}
