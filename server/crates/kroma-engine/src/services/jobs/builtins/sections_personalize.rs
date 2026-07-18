//! `sections.personalize` the LLM-powered personalization pass: for every user
//! with enough watch history, cluster their taste, ask the configured LLM to name
//! a few sections + refine their taste profile, and cache the result. Served on
//! the home screen by [`crate::services::sections::build_home`]. Heavy + nightly.

use super::prelude::*;

/// Nightly: name per-account taste clusters into personalized rows.
pub(super) const SPEC: Builtin = Builtin {
    key: JobKey("sections.personalize"),
    category: Category::Recommendations,
    schedule: Some("30 5 * * *"),
    triggers: &[],
    run,
};

/// How many taste clusters (→ ~that many named sections) per user.
const PERSONALIZE_CLUSTERS: usize = 4;

pub(super) fn run(ctx: &JobContext) -> Result<()> {
    use crate::infra::events::ServerEvent;
    use crate::services::sections::{generate, taste};

    let state = &ctx.state;
    let llm = crate::infra::llm::from_settings(&state.settings);
    if !llm.available() {
        ctx.warn("no LLM configured enable one under Admin → Général → IA; skipping");
        return Ok(());
    }
    ctx.info(format!("personalizing with {}", llm.describe()));
    // Output cap from the default provider (per-provider since multi-provider).
    let max_tokens = crate::services::settings::default_provider(&state.settings)
        .map(|p| p.max_tokens)
        .unwrap_or(900)
        .clamp(64, 8192) as u32;
    state.vectors.refresh_if_stale(&state.db)?;

    let users = crate::db::all_users_with_lang(&state.db)?;
    let total = users.len();
    let mut generated = 0usize;
    for (i, (uid, lang)) in users.iter().enumerate() {
        if ctx.cancelled() {
            ctx.warn("cancellation requested stopping");
            break;
        }
        ctx.progress(i + 1, total);

        let watched = crate::db::recent_watched_ids(&state.db, uid).unwrap_or_default();
        let clusters = taste::cluster(&state.db, &state.vectors, &watched, PERSONALIZE_CLUSTERS);
        if clusters.is_empty() {
            let embedded = state.vectors.vectors_for(&watched).len();
            ctx.debug(format!(
                "{}: {} watched / {} with embeddings (< {} required) skipping",
                short_id(uid), watched.len(), embedded, taste::MIN_WATCHED
            ));
            continue;
        }
        let locale = lang.as_deref().and_then(crate::i18n::normalize).unwrap_or(crate::i18n::DEFAULT_LOCALE);
        let prev = crate::db::get_user_taste(&state.db, uid).ok().flatten().and_then(|(p, _)| p);
        let (system, user) = generate::build_prompt(locale, prev.as_deref(), &clusters);
        ctx.debug(format!(
            "{}: {} taste clusters → {} ({} prompt chars)",
            short_id(uid), clusters.len(), llm.describe(), system.len() + user.len()
        ));

        match llm.complete(&system, &user, max_tokens) {
            Ok(reply) => match generate::parse_response(&reply) {
                Ok((profile, sections)) if !sections.is_empty() => {
                    let json = serde_json::to_string(&sections).unwrap_or_else(|_| "[]".into());
                    crate::db::set_user_taste(&state.db, uid, Some(&profile), &json)?;
                    generated += 1;
                    ctx.info(format!("{}: {} sections", short_id(uid), sections.len()));
                }
                Ok(_) => ctx.error(format!(
                    "{}: model returned no usable sections reply: {}", short_id(uid), snippet(&reply)
                )),
                Err(e) => ctx.error(format!(
                    "{}: could not parse model reply: {e} reply: {}", short_id(uid), snippet(&reply)
                )),
            },
            Err(e) => ctx.error(format!("{}: LLM request failed: {e:#}", short_id(uid))),
        }
    }

    ctx.info(format!("personalized {generated}/{total} users"));
    // Tell live clients to refetch the home screen with their new rows.
    state.events.publish(ServerEvent::LibraryUpdated);
    Ok(())
}

/// First 8 chars of an id, for compact log lines.
fn short_id(id: &str) -> &str {
    &id[..id.len().min(8)]
}

#[cfg(test)]
mod tests {
    use super::short_id;

    #[test]
    fn short_id_truncates_to_eight_bytes() {
        assert_eq!(short_id("0123456789abcdef"), "01234567");
        // Ids at or under 8 bytes are returned whole.
        assert_eq!(short_id("abc"), "abc");
        assert_eq!(short_id("01234567"), "01234567");
        assert_eq!(short_id(""), "");
    }
}
