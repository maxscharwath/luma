//! Optional password encryption for backup archives.
//!
//! A password wraps the ZIP bytes in a small AEAD envelope: ChaCha20-Poly1305
//! (software, constant-time, no AES-NI needed on a NAS) with the key derived from
//! the project's own PBKDF2-HMAC-SHA256 ([`crate::services::auth`]) and a
//! `/dev/urandom` salt+nonce. The fixed-size header is authenticated as AAD, so a
//! tampered KDF param or truncation fails the tag check which is also how a
//! wrong password surfaces (the Poly1305 tag won't verify).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

use crate::services::auth::{pbkdf2_sha256, random_bytes};

/// Envelope magic (8 bytes, trailing `\n` so a text viewer shows one line).
const MAGIC: &[u8; 8] = b"LUMABK1\n";
/// Envelope version.
const VER: u8 = 1;
/// KDF id: 1 = PBKDF2-HMAC-SHA256.
const KDF_PBKDF2: u8 = 1;
/// PBKDF2 iterations for the backup key (independent of the login KDF).
const ITERS: u32 = 210_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
/// Bytes before the ciphertext: magic(8)+ver(1)+kdf(1)+iters(4)+salt(16)+nonce(12).
const HEADER_LEN: usize = 8 + 1 + 1 + 4 + SALT_LEN + NONCE_LEN;

/// True if `bytes` is an encrypted backup envelope.
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() && &bytes[..MAGIC.len()] == MAGIC
}

/// Encrypt `plaintext` (the ZIP bytes) under `password` → the envelope bytes.
pub fn seal(plaintext: &[u8], password: &str) -> anyhow::Result<Vec<u8>> {
    let salt = random_bytes(SALT_LEN);
    let nonce = random_bytes(NONCE_LEN);
    let key = pbkdf2_sha256(password.as_bytes(), &salt, ITERS);

    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(VER);
    header.push(KDF_PBKDF2);
    header.extend_from_slice(&ITERS.to_be_bytes());
    header.extend_from_slice(&salt);
    header.extend_from_slice(&nonce);

    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| anyhow::anyhow!("backup key init failed"))?;
    let nonce_ga = Nonce::try_from(&nonce[..]).expect("NONCE_LEN-byte nonce");
    let ct = cipher
        .encrypt(&nonce_ga, Payload { msg: plaintext, aad: &header })
        .map_err(|_| anyhow::anyhow!("backup encryption failed"))?;

    let mut out = header;
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt an envelope under `password` → the ZIP bytes. `Ok(None)` means the
/// password is wrong or the data is corrupted (the AEAD tag did not verify);
/// `Err` means the envelope is structurally invalid.
pub fn open(bytes: &[u8], password: &str) -> anyhow::Result<Option<Vec<u8>>> {
    if !is_encrypted(bytes) || bytes.len() < HEADER_LEN {
        anyhow::bail!("not a LUMA encrypted backup");
    }
    let ver = bytes[8];
    let kdf = bytes[9];
    if ver != VER || kdf != KDF_PBKDF2 {
        anyhow::bail!("unsupported backup envelope (ver {ver}, kdf {kdf})");
    }
    let iters = u32::from_be_bytes(bytes[10..14].try_into().unwrap());
    let salt = &bytes[14..14 + SALT_LEN];
    let nonce = &bytes[14 + SALT_LEN..HEADER_LEN];
    let header = &bytes[..HEADER_LEN];

    let key = pbkdf2_sha256(password.as_bytes(), salt, iters);
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| anyhow::anyhow!("backup key init failed"))?;
    let nonce_ga = Nonce::try_from(nonce).expect("NONCE_LEN-byte nonce");
    Ok(cipher
        .decrypt(&nonce_ga, Payload { msg: &bytes[HEADER_LEN..], aad: header })
        .ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_round_trip_and_reject_wrong_password() {
        let data = b"PK\x03\x04 fake zip bytes ...".to_vec();
        let sealed = seal(&data, "correct horse").unwrap();
        assert!(is_encrypted(&sealed));
        assert_ne!(&sealed[HEADER_LEN..], &data[..], "ciphertext differs from plaintext");

        assert_eq!(open(&sealed, "correct horse").unwrap().as_deref(), Some(&data[..]));
        // Wrong password → tag fails → None (not an Err, not garbage).
        assert_eq!(open(&sealed, "wrong").unwrap(), None);
        // A flipped ciphertext byte → tamper detected.
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 0x01;
        assert_eq!(open(&tampered, "correct horse").unwrap(), None);
    }
}
