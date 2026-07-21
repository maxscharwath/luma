//! Optional HTTPS listener with an auto-generated self-signed certificate.
//!
//! On a local network the browser refuses the Web Crypto API (`crypto.subtle`,
//! WebAuthn/passkeys) on a plain-HTTP origin that isn't `localhost`, so a phone
//! or a second machine hitting `http://192.168.x.y:4040` can't use those. Serving
//! HTTPS fixes that, but a LAN box has no public DNS name for Let's Encrypt. The
//! pragmatic answer is a self-signed cert: generated once, persisted under
//! `<data>/tls/`, covering the machine's LAN identities (localhost, hostname,
//! `<hostname>.local`, and the primary LAN IP). The user trusts it once per
//! device (or downloads it from `/api/tls/cert.pem`).
//!
//! The plain-HTTP listener keeps serving in parallel; HTTPS is additive and off
//! by default (the `httpsEnabled` setting / `KROMA_HTTPS` env).

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The certificate + key file paths inside `<data>/tls/`.
pub struct CertPaths {
    pub cert_pem: PathBuf,
    pub key_pem: PathBuf,
}

/// Ensure the rustls process-default crypto provider is installed (ring). More
/// than one provider is compiled into the dependency graph (ring + aws-lc-rs via
/// transitive deps), so rustls has no implicit default and would panic on first
/// use; pin ring explicitly (C-free, matches the musl build policy). Idempotent:
/// a second call is a no-op once one is installed.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Return the persisted self-signed cert, generating (and writing) it the first
/// time. Regenerates if either file is missing so a half-written pair self-heals.
pub fn ensure_self_signed(tls_dir: &Path, extra_sans: &[String]) -> Result<CertPaths> {
    let cert_pem = tls_dir.join("cert.pem");
    let key_pem = tls_dir.join("key.pem");
    if cert_pem.is_file() && key_pem.is_file() {
        return Ok(CertPaths { cert_pem, key_pem });
    }

    std::fs::create_dir_all(tls_dir)
        .with_context(|| format!("failed to create TLS dir {}", tls_dir.display()))?;

    let (cert, key) = generate(extra_sans)?;
    std::fs::write(&cert_pem, cert.as_bytes())
        .with_context(|| format!("failed to write {}", cert_pem.display()))?;
    write_private(&key_pem, key.as_bytes())
        .with_context(|| format!("failed to write {}", key_pem.display()))?;

    tracing::info!(cert = %cert_pem.display(), "generated a self-signed TLS certificate");
    Ok(CertPaths { cert_pem, key_pem })
}

/// Write a private key with owner-only permissions where the platform supports
/// it (0600 on unix), so the key isn't world-readable on a shared box.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Generate a fresh self-signed cert + key (PEM), with SANs covering the box's
/// local identities plus any admin-supplied extras.
fn generate(extra_sans: &[String]) -> Result<(String, String)> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    let mut params = CertificateParams::default();
    params.distinguished_name = {
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "KROMA");
        dn.push(DnType::OrganizationName, "KROMA");
        dn
    };
    // A wide, fixed validity window (no wall-clock read needed, and the user
    // trusts the fingerprint once): valid from 2024 through 2035.
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2035, 1, 1);
    params.subject_alt_names = collect_sans(extra_sans);

    let key = KeyPair::generate().context("failed to generate a TLS key pair")?;
    let cert = params.self_signed(&key).context("failed to self-sign the TLS certificate")?;
    Ok((cert.pem(), key.serialize_pem()))
}

/// Build the SAN list: always localhost + loopback, plus the machine hostname,
/// its `.local` mDNS name, the primary LAN IP, and any admin extras (an IP or a
/// DNS name; parsed as an IP when possible, else a DNS name). Deduplicated.
fn collect_sans(extra: &[String]) -> Vec<rcgen::SanType> {
    use rcgen::SanType;

    let mut dns: Vec<String> = vec!["localhost".to_string()];
    let mut ips: Vec<IpAddr> = vec![IpAddr::from([127, 0, 0, 1]), IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1])];

    if let Ok(host) = hostname::get() {
        let host = host.to_string_lossy().trim().to_string();
        if !host.is_empty() && host != "localhost" {
            // The `.local` mDNS name lets a client reach the box by name without
            // DNS (the mDNS module advertises it).
            let local = format!("{}.local", host.split('.').next().unwrap_or(&host));
            dns.push(host);
            dns.push(local);
        }
    }

    if let Some(ip) = primary_lan_ip() {
        ips.push(ip);
    }

    for s in extra {
        match s.parse::<IpAddr>() {
            Ok(ip) => ips.push(ip),
            Err(_) => dns.push(s.clone()),
        }
    }

    dns.sort();
    dns.dedup();
    ips.sort();
    ips.dedup();

    let mut out: Vec<SanType> = Vec::new();
    for d in dns {
        if let Ok(name) = d.clone().try_into() {
            out.push(SanType::DnsName(name));
        }
    }
    out.extend(ips.into_iter().map(SanType::IpAddress));
    out
}

/// The box's primary LAN IP, via the classic "connect a UDP socket to a public
/// address and read back the local endpoint the OS picked" trick (no packet is
/// sent for a UDP connect; it just resolves the default-route source address).
/// `None` when the host is offline / has no default route.
fn primary_lan_ip() -> Option<IpAddr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.168.255.255:9").or_else(|_| sock.connect("8.8.8.8:53")).ok()?;
    sock.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

/// The HTTPS socket address to bind, from the config/host + resolved port.
pub fn https_addr(host: &str, port: u16) -> SocketAddr {
    let ip: IpAddr = host.parse().unwrap_or(IpAddr::from([0, 0, 0, 0]));
    SocketAddr::new(ip, port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_always_include_localhost_and_loopback() {
        let sans = collect_sans(&[]);
        let has_localhost = sans.iter().any(|s| matches!(s, rcgen::SanType::DnsName(n) if n.as_str() == "localhost"));
        let has_loopback = sans
            .iter()
            .any(|s| matches!(s, rcgen::SanType::IpAddress(ip) if ip == &IpAddr::from([127, 0, 0, 1])));
        assert!(has_localhost, "localhost DNS SAN missing");
        assert!(has_loopback, "127.0.0.1 IP SAN missing");
    }

    #[test]
    fn extra_sans_split_ip_vs_dns() {
        let sans = collect_sans(&["10.1.2.3".to_string(), "nas.home".to_string()]);
        assert!(sans
            .iter()
            .any(|s| matches!(s, rcgen::SanType::IpAddress(ip) if ip == &IpAddr::from([10, 1, 2, 3]))));
        assert!(sans
            .iter()
            .any(|s| matches!(s, rcgen::SanType::DnsName(n) if n.as_str() == "nas.home")));
    }

    #[test]
    fn generate_produces_pem_pair() {
        let (cert, key) = generate(&[]).expect("generate");
        assert!(cert.contains("BEGIN CERTIFICATE"));
        assert!(key.contains("PRIVATE KEY"));
    }

    #[test]
    fn ensure_self_signed_persists_and_reuses() {
        let dir = std::env::temp_dir().join(format!("kroma-tls-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = ensure_self_signed(&dir, &[]).expect("first");
        let cert1 = std::fs::read(&a.cert_pem).unwrap();
        let b = ensure_self_signed(&dir, &[]).expect("second");
        let cert2 = std::fs::read(&b.cert_pem).unwrap();
        // Reused, not regenerated: identical bytes on the second call.
        assert_eq!(cert1, cert2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
