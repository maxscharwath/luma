//! Module <-> server compatibility checks, shared by the supervisor (install /
//! spawn gate) and the store endpoints (catalog "compatible" flag).
//!
//! Both helpers are PERMISSIVE, mirroring the dependency-range policy in
//! [`crate::Registry`]: an unparseable declaration or version is treated as
//! "no constraint" rather than a mismatch, so a typo'd manifest degrades to
//! installable instead of bricking the module everywhere.

/// Whether `server_version` satisfies a module's `minServer` declaration.
/// `min_server` accepts a bare version (`"0.2.0"`, meaning "at least 0.2.0")
/// or a full dtolnay semver range (`">=0.2, <0.4"`). `None`, blank and `"*"`
/// all mean "any server".
pub fn server_satisfies(min_server: Option<&str>, server_version: &str) -> bool {
    let Some(decl) = min_server.map(str::trim).filter(|s| !s.is_empty() && *s != "*") else {
        return true;
    };
    let Ok(server) = semver::Version::parse(server_version) else {
        return true;
    };
    if let Ok(min) = semver::Version::parse(decl) {
        return server >= min;
    }
    if let Ok(req) = semver::VersionReq::parse(decl) {
        return req.matches(&server);
    }
    true
}

/// Whether `version` satisfies a dependency `range` (dtolnay semver syntax).
/// Blank / `"*"` ranges match anything; unparseable inputs never block.
pub fn range_matches(range: &str, version: &str) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    match (semver::VersionReq::parse(range), semver::Version::parse(version)) {
        (Ok(req), Ok(v)) => req.matches(&v),
        _ => true,
    }
}

/// Whether `candidate` is a strictly newer version than `current` (the store's
/// "update available" test). Falls back to plain inequality when either side
/// is not semver, so a registry with odd versions still surfaces changes.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (semver::Version::parse(candidate.trim()), semver::Version::parse(current.trim())) {
        (Ok(c), Ok(i)) => c > i,
        _ => candidate.trim() != current.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_compares_semver_then_falls_back() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(is_newer("nightly-2", "nightly-1"));
    }

    #[test]
    fn bare_min_server_means_at_least() {
        assert!(server_satisfies(Some("0.1.4"), "0.1.4"));
        assert!(server_satisfies(Some("0.1.4"), "0.2.0"));
        assert!(!server_satisfies(Some("0.2.0"), "0.1.4"));
    }

    #[test]
    fn ranges_and_wildcards_work() {
        assert!(server_satisfies(Some(">=0.1, <0.3"), "0.2.9"));
        assert!(!server_satisfies(Some(">=0.3"), "0.2.9"));
        assert!(server_satisfies(Some("*"), "0.0.1"));
        assert!(server_satisfies(Some("  "), "0.0.1"));
        assert!(server_satisfies(None, "0.0.1"));
    }

    #[test]
    fn unparseable_declarations_never_block() {
        assert!(server_satisfies(Some("not-a-version"), "0.1.4"));
        assert!(server_satisfies(Some("1.0.0"), "not-a-version"));
        assert!(range_matches("garbage", "0.1.0"));
    }

    #[test]
    fn dependency_ranges() {
        assert!(range_matches("^0.1.0", "0.1.9"));
        assert!(!range_matches("^0.1.0", "0.2.0"));
        assert!(range_matches("*", "9.9.9"));
    }
}
