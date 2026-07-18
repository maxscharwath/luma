//! LAN/WAN classification of a client IP, against the configured local networks.

use std::net::IpAddr;

/// Classify a client IP as `LAN` or `WAN` against the configured local networks
/// (CIDR `a.b.c.d/n` or a bare prefix like `192.168.`). Loopback is always LAN.
pub fn classify_network(ip: &str, local_nets: &[String]) -> String {
    let Ok(addr) = ip.parse::<IpAddr>() else {
        return "WAN".into();
    };
    if addr.is_loopback() {
        return "LAN".into();
    }
    // RFC1918 / link-local are LAN regardless of config.
    if is_private(&addr) {
        return "LAN".into();
    }
    for net in local_nets {
        if cidr_contains(net, &addr) {
            return "LAN".into();
        }
    }
    "WAN".into()
}

fn is_private(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

/// Minimal IPv4 CIDR / prefix match. Accepts `a.b.c.d/n` and bare `a.b.` prefixes.
fn cidr_contains(net: &str, addr: &IpAddr) -> bool {
    let IpAddr::V4(ip) = addr else { return false };
    let ip = u32::from(*ip);
    if let Some((base, bits)) = net.split_once('/') {
        let Ok(base_ip) = base.trim().parse::<std::net::Ipv4Addr>() else {
            return false;
        };
        let Ok(bits) = bits.trim().parse::<u32>() else {
            return false;
        };
        if bits == 0 {
            return true;
        }
        if bits > 32 {
            return false;
        }
        let mask = u32::MAX << (32 - bits);
        (u32::from(base_ip) & mask) == (ip & mask)
    } else {
        // Bare prefix string match on the dotted form.
        std::net::Ipv4Addr::from(ip).to_string().starts_with(net.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nets() -> Vec<String> {
        vec!["203.0.113.0/24".to_string(), "198.51.100.".to_string()]
    }

    #[test]
    fn loopback_and_private_are_lan_regardless_of_config() {
        assert_eq!(classify_network("127.0.0.1", &[]), "LAN");
        assert_eq!(classify_network("::1", &[]), "LAN");
        assert_eq!(classify_network("192.168.1.10", &[]), "LAN");
        assert_eq!(classify_network("10.1.2.3", &[]), "LAN");
        assert_eq!(classify_network("172.16.5.5", &[]), "LAN");
        assert_eq!(classify_network("169.254.1.1", &[]), "LAN"); // link-local
    }

    #[test]
    fn public_ip_is_wan_unless_in_configured_net() {
        assert_eq!(classify_network("8.8.8.8", &nets()), "WAN");
        // Inside the configured CIDR.
        assert_eq!(classify_network("203.0.113.42", &nets()), "LAN");
        // Inside the bare-prefix net.
        assert_eq!(classify_network("198.51.100.7", &nets()), "LAN");
        // Just outside both.
        assert_eq!(classify_network("203.0.114.1", &nets()), "WAN");
    }

    #[test]
    fn invalid_ip_is_wan() {
        assert_eq!(classify_network("not-an-ip", &nets()), "WAN");
        assert_eq!(classify_network("", &nets()), "WAN");
    }

    #[test]
    fn ipv6_ula_is_lan_global_is_wan() {
        assert_eq!(classify_network("fc00::1", &[]), "LAN"); // unique-local
        assert_eq!(classify_network("fd12:3456::1", &[]), "LAN");
        assert_eq!(classify_network("2001:4860:4860::8888", &[]), "WAN"); // global
    }

    #[test]
    fn cidr_contains_edge_cases() {
        let a: IpAddr = "10.0.0.5".parse().unwrap();
        // /0 always matches.
        assert!(cidr_contains("0.0.0.0/0", &a));
        // >32 bits is rejected.
        assert!(!cidr_contains("10.0.0.0/33", &a));
        // Bad base or bits parse -> false.
        assert!(!cidr_contains("notanip/8", &a));
        assert!(!cidr_contains("10.0.0.0/xx", &a));
        // Exact /32 match.
        assert!(cidr_contains("10.0.0.5/32", &a));
        assert!(!cidr_contains("10.0.0.6/32", &a));
        // IPv6 addr never matches an IPv4 CIDR.
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        assert!(!cidr_contains("10.0.0.0/8", &v6));
    }

    #[test]
    fn is_private_classifies_v4_and_v6() {
        assert!(is_private(&"192.168.0.1".parse().unwrap()));
        assert!(is_private(&"169.254.0.1".parse().unwrap()));
        assert!(!is_private(&"8.8.8.8".parse().unwrap()));
        assert!(is_private(&"fc00::1".parse().unwrap()));
        assert!(!is_private(&"2001:db8::1".parse().unwrap()));
    }
}
