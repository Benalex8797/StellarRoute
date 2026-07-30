//! One-time transfer access capability tokens (bearer, stored as hash only).

use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::cctp::bounds::check_str_len;

pub const TRANSFER_ACCESS_HEADER: &str = "x-cctp-transfer-access";
pub const ACCESS_TOKEN_BYTES: usize = 32;
pub const MAX_ACCESS_TOKEN_LEN: usize = 128;

/// Generate a high-entropy access token and its SHA-256 hex digest for persistence.
pub fn generate_access_token() -> (String, String) {
    let mut raw = [0u8; ACCESS_TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let hash = hash_access_token(&token);
    (token, hash)
}

pub fn hash_access_token(token: &str) -> String {
    let digest = Sha256::digest(token.trim().as_bytes());
    hex::encode(digest)
}

pub fn validate_access_token_format(token: &str) -> Result<(), String> {
    let trimmed = token.trim();
    check_str_len("access_token", trimmed, MAX_ACCESS_TOKEN_LEN)?;
    if trimmed.len() != ACCESS_TOKEN_BYTES * 2 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("access token must be 64 hex characters".into());
    }
    Ok(())
}

pub fn access_tokens_match(persisted_hash: &str, presented: &str) -> bool {
    validate_access_token_format(presented).is_ok()
        && constant_time_eq(
            persisted_hash.as_bytes(),
            hash_access_token(presented).as_bytes(),
        )
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn test_access_token_hash() -> String {
    hash_access_token("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip_hash() {
        let (token, hash) = generate_access_token();
        assert_eq!(hash.len(), 64);
        assert!(access_tokens_match(&hash, &token));
        assert!(!access_tokens_match(&hash, "deadbeef"));
    }
}
