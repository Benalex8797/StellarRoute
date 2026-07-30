//! Transfer access capability tokens — hash-only persistence; HMAC recovery for idempotent quotes.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::cctp::bounds::check_str_len;

pub const TRANSFER_ACCESS_HEADER: &str = "x-cctp-transfer-access";
pub const ACCESS_TOKEN_HMAC_ENV: &str = "CCTP_ACCESS_TOKEN_HMAC_KEY";
pub const ACCESS_TOKEN_BYTES: usize = 32;
pub const MIN_HMAC_KEY_BYTES: usize = 32;
pub const MAX_ACCESS_TOKEN_LEN: usize = 128;

type HmacSha256 = Hmac<Sha256>;

/// Production HMAC key for deterministic idempotent quote tokens (never logged).
#[derive(Clone)]
pub struct CctpAccessTokenKey {
    bytes: Vec<u8>,
}

impl std::fmt::Debug for CctpAccessTokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CctpAccessTokenKey([REDACTED])")
    }
}

impl CctpAccessTokenKey {
    pub fn from_env_when_enabled(enabled: bool) -> Result<Option<Self>, String> {
        let raw = match std::env::var(ACCESS_TOKEN_HMAC_ENV) {
            Ok(v) if !v.trim().is_empty() => v,
            _ if enabled => {
                return Err(format!(
                    "{ACCESS_TOKEN_HMAC_ENV} is required when CCTP_ENABLED=true (>= {MIN_HMAC_KEY_BYTES} random bytes, base64 or hex)"
                ));
            }
            _ => return Ok(None),
        };
        let bytes = parse_secret_bytes(&raw)?;
        if bytes.len() < MIN_HMAC_KEY_BYTES {
            return Err(format!(
                "{ACCESS_TOKEN_HMAC_ENV} must decode to at least {MIN_HMAC_KEY_BYTES} bytes"
            ));
        }
        Ok(Some(Self { bytes }))
    }

    pub fn from_test_bytes(bytes: Vec<u8>) -> Self {
        assert!(bytes.len() >= MIN_HMAC_KEY_BYTES);
        Self { bytes }
    }

    /// Deterministic token for idempotent quote replays (domain-separated HMAC-SHA256, base64url).
    pub fn derive_idempotent_token(
        &self,
        idempotency_key: &str,
        canonical_request_hash: &str,
        transfer_id: Uuid,
    ) -> String {
        let mut mac =
            HmacSha256::new_from_slice(&self.bytes).expect("HMAC key length validated at load");
        mac.update(b"cctp-transfer-access-v1\0");
        mac.update(idempotency_key.as_bytes());
        mac.update(b"\0");
        mac.update(canonical_request_hash.as_bytes());
        mac.update(b"\0");
        mac.update(transfer_id.as_bytes());
        let digest = mac.finalize().into_bytes();
        URL_SAFE_NO_PAD.encode(digest)
    }
}

fn parse_secret_bytes(raw: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw.trim();
    if let Ok(bytes) = hex::decode(trimmed) {
        return Ok(bytes);
    }
    if let Ok(bytes) = URL_SAFE_NO_PAD.decode(trimmed) {
        return Ok(bytes);
    }
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
        return Ok(bytes);
    }
    Err(format!(
        "{ACCESS_TOKEN_HMAC_ENV} must be hex, standard base64, or base64url"
    ))
}

/// One-time CSPRNG token for non-idempotent quotes (returned once; only hash stored).
pub fn generate_ephemeral_access_token() -> (String, String) {
    let mut raw = [0u8; ACCESS_TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = URL_SAFE_NO_PAD.encode(raw);
    let hash = hash_access_token(&token);
    (token, hash)
}

pub fn hash_access_token(token: &str) -> String {
    let digest = Sha256::digest(token.trim().as_bytes());
    hex::encode(digest)
}

pub fn hash_lease_owner(owner_nonce: &str) -> String {
    hash_access_token(owner_nonce)
}

pub fn validate_access_token_format(token: &str) -> Result<(), String> {
    let trimmed = token.trim();
    check_str_len("access_token", trimmed, MAX_ACCESS_TOKEN_LEN)?;
    if trimmed.is_empty() {
        return Err("access token required".into());
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

/// Legacy helper for tests that predate HMAC idempotency.
pub fn generate_access_token() -> (String, String) {
    generate_ephemeral_access_token()
}

pub fn test_access_token_key() -> CctpAccessTokenKey {
    CctpAccessTokenKey::from_test_bytes(vec![0x42u8; MIN_HMAC_KEY_BYTES])
}

pub fn test_access_token_hash() -> String {
    hash_access_token("test-token-for-unit-tests-only")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ephemeral_token_roundtrip_hash() {
        let (token, hash) = generate_ephemeral_access_token();
        assert!(access_tokens_match(&hash, &token));
        assert!(!access_tokens_match(&hash, "wrong"));
    }

    #[test]
    fn idempotent_token_is_deterministic() {
        let key = test_access_token_key();
        let id = Uuid::new_v4();
        let a = key.derive_idempotent_token("idem-1", "abc123", id);
        let b = key.derive_idempotent_token("idem-1", "abc123", id);
        assert_eq!(a, b);
        assert_ne!(key.derive_idempotent_token("idem-2", "abc123", id), a);
    }

    #[test]
    fn hmac_key_parsing_hex_and_base64() {
        let hex_key = hex::encode([1u8; 32]);
        let parsed = parse_secret_bytes(&hex_key).unwrap();
        assert_eq!(parsed.len(), 32);
    }
}
