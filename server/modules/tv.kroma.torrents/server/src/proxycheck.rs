//! VPN reachability probe for the kill switch: fetch an IP-echo endpoint
//! through the SOCKS5 proxy, plus (best-effort) directly, and compare. A dead
//! proxy or a proxy that is not actually diverting traffic both read as "not
//! sealed".

/// Outcome of one probe.
#[derive(Debug, Clone, Default)]
pub struct VpnCheck {
    /// Exit IP as seen through the proxy; `None` = the proxy is unreachable.
    pub proxied_ip: Option<String>,
    /// Exit IP without the proxy (best-effort; boxes with VPN-only egress
    /// legitimately have none).
    pub direct_ip: Option<String>,
    pub error: Option<String>,
}

impl VpnCheck {
    /// The gate: proxied egress works AND (when a direct comparison exists)
    /// actually differs from the direct route.
    pub fn sealed(&self) -> bool {
        match (&self.proxied_ip, &self.direct_ip) {
            (Some(p), Some(d)) => p != d,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

fn short_ip(body: String) -> Option<String> {
    let trimmed = body.trim();
    // IP echo endpoints answer a bare address; anything page-sized is a
    // captive portal / error page, not an IP.
    (!trimmed.is_empty() && trimmed.len() <= 64).then(|| trimmed.to_string())
}

/// Probe `check_url` through `proxy` (`socks5://[user:pass@]host:port` or
/// `host:port`) and directly.
pub fn check(proxy: &str, check_url: &str) -> VpnCheck {
    let proxied = kroma_module_sdk::http::Fetch::new()
        .max_time(12)
        .socks5(proxy.trim().trim_start_matches("socks5://"))
        .get(check_url)
        .and_then(|r| r.ensure_ok())
        .map(|r| r.text());
    let direct = kroma_module_sdk::http::Fetch::new()
        .max_time(12)
        .get(check_url)
        .and_then(|r| r.ensure_ok())
        .map(|r| r.text());
    let mut out = VpnCheck::default();
    match proxied {
        Ok(body) => out.proxied_ip = short_ip(body),
        Err(e) => out.error = Some(format!("{e:#}")),
    }
    if let Ok(body) = direct {
        out.direct_ip = short_ip(body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_logic() {
        let mk = |p: Option<&str>, d: Option<&str>| VpnCheck {
            proxied_ip: p.map(str::to_string),
            direct_ip: d.map(str::to_string),
            error: None,
        };
        assert!(mk(Some("1.2.3.4"), Some("5.6.7.8")).sealed());
        assert!(mk(Some("1.2.3.4"), None).sealed(), "no direct egress still counts");
        assert!(!mk(None, Some("5.6.7.8")).sealed(), "dead proxy is not sealed");
        assert!(!mk(Some("5.6.7.8"), Some("5.6.7.8")).sealed(), "same exit = not diverting");
    }

    #[test]
    fn ip_echo_bodies_are_size_capped() {
        assert_eq!(short_ip("  203.0.113.7\n".into()).as_deref(), Some("203.0.113.7"));
        assert_eq!(short_ip("<html>captive portal</html>".repeat(10)), None);
        assert_eq!(short_ip("   ".into()), None);
    }
}
