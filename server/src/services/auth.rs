//! Authentication primitives: password hashing and session tokens.
//!
//! Crypto is hand-rolled on top of the `sha2` crate already in the dependency
//! tree (PBKDF2-HMAC-SHA256), keeping the build lean and 1.81-friendly the
//! same reasoning that led the project to hand-roll its SQLite pool rather than
//! pull `r2d2_sqlite` → uuid → rand. Randomness comes from `/dev/urandom`
//! (the server only ever runs on Unix: Linux NAS / macOS dev).

use sha2::{Digest, Sha256};

/// PBKDF2 iteration count. A balance between the OWASP recommendation and the
/// modest CPU of a NAS login stays well under a second.
const PBKDF2_ITERS: u32 = 120_000;
/// Salt length in bytes.
const SALT_LEN: usize = 16;
/// SHA-256 block size (HMAC).
const SHA256_BLOCK: usize = 64;
/// Short-lived session (bearer) token lifetime the client refreshes it from its
/// access token before/after this lapses (see `/auth/token`).
pub const SESSION_TTL_SECS: i64 = 3600;
/// Long-lived access-token lifetime (90 days). Stored on the device; exchanged
/// for session tokens. This is the credential a logout revokes.
pub const ACCESS_TTL_SECS: i64 = 90 * 24 * 3600;

// ----- HMAC / PBKDF2 ----------------------------------------------------------

/// HMAC-SHA256 (RFC 2104) over `msg` keyed by `key`.
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; SHA256_BLOCK];
    if key.len() > SHA256_BLOCK {
        let mut h = Sha256::new();
        h.update(key);
        k[..32].copy_from_slice(&h.finalize());
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; SHA256_BLOCK];
    let mut opad = [0x5cu8; SHA256_BLOCK];
    for i in 0..SHA256_BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);

    let mut out = [0u8; 32];
    out.copy_from_slice(&outer.finalize());
    out
}

/// PBKDF2-HMAC-SHA256 producing a single 32-byte derived key (dkLen == hLen, so
/// exactly one block is needed INT(i) is always `0x00000001`). `pub(crate)` so
/// the backup envelope derives its encryption key from the same KDF.
pub(crate) fn pbkdf2_sha256(password: &[u8], salt: &[u8], iters: u32) -> [u8; 32] {
    let mut block = Vec::with_capacity(salt.len() + 4);
    block.extend_from_slice(salt);
    block.extend_from_slice(&1u32.to_be_bytes());

    let mut u = hmac_sha256(password, &block);
    let mut out = u;
    for _ in 1..iters {
        u = hmac_sha256(password, &u);
        for i in 0..32 {
            out[i] ^= u[i];
        }
    }
    out
}

/// Constant-time byte comparison (avoids leaking the match prefix via timing).
pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

// ----- randomness -------------------------------------------------------------

/// `n` cryptographically-random bytes from `/dev/urandom`. Falls back to a
/// time-seeded SHA-256 stream only if the device is unreadable (never on a sane
/// Unix host) so token issuance can't hard-fail. `pub(crate)` reused for backup
/// envelope salts/nonces.
pub(crate) fn random_bytes(n: usize) -> Vec<u8> {
    use std::io::Read;
    let mut buf = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    // Degraded fallback: SHA-256 CTR seeded with the high-resolution clock.
    let seed = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let mut out = Vec::with_capacity(n);
    let mut counter: u64 = 0;
    while out.len() < n {
        let mut h = Sha256::new();
        h.update(seed.to_le_bytes());
        h.update(counter.to_le_bytes());
        out.extend_from_slice(&h.finalize());
        counter += 1;
    }
    out.truncate(n);
    out
}

/// A fresh opaque session token: 32 random bytes, hex-encoded (64 chars).
pub fn random_token() -> String {
    hex::encode(random_bytes(32))
}

/// A random `u32` (used to pick Quick Connect numeric codes).
pub fn random_u32() -> u32 {
    let b = random_bytes(4);
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

// ----- password hashing -------------------------------------------------------

/// Hash a plaintext password to the storable form `pbkdf2$<iters>$<salt_hex>$<dk_hex>`.
pub fn hash_password(password: &str) -> String {
    let salt = random_bytes(SALT_LEN);
    let dk = pbkdf2_sha256(password.as_bytes(), &salt, PBKDF2_ITERS);
    format!("pbkdf2${PBKDF2_ITERS}${}${}", hex::encode(&salt), hex::encode(dk))
}

/// Verify `password` against a stored `pbkdf2$…` hash. Returns false on any
/// malformed hash.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let mut parts = stored.split('$');
    if parts.next() != Some("pbkdf2") {
        return false;
    }
    let Some(iters) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    let Some(salt) = parts.next().and_then(|s| hex::decode(s).ok()) else {
        return false;
    };
    let Some(expected) = parts.next().and_then(|s| hex::decode(s).ok()) else {
        return false;
    };
    let dk = pbkdf2_sha256(password.as_bytes(), &salt, iters);
    ct_eq(&dk, &expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_known_vector() {
        // RFC 6070-style vector for PBKDF2-HMAC-SHA256:
        // P="password", S="salt", c=1, dkLen=32.
        let dk = pbkdf2_sha256(b"password", b"salt", 1);
        assert_eq!(
            hex::encode(dk),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        // c=2 vector.
        let dk2 = pbkdf2_sha256(b"password", b"salt", 2);
        assert_eq!(
            hex::encode(dk2),
            "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
        );
    }

    #[test]
    fn hmac_known_vector() {
        // RFC 4231 test case 2: key="Jefe", data="what do ya want for nothing?".
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex::encode(mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hash_round_trip() {
        let h = hash_password("s3cret!");
        assert!(h.starts_with("pbkdf2$"));
        assert!(verify_password("s3cret!", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("s3cret!", "garbage"));
    }

    #[test]
    fn tokens_are_unique_and_long() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
