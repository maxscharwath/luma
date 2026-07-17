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
