//! Size limits for untrusted CCTP / Iris payloads.

pub const MAX_RAW_MESSAGE_BYTES: usize = 8_192;
pub const MAX_ATTESTATION_BYTES: usize = 8_192;
pub const MAX_HOOK_DATA_BYTES: usize = 4_096;
pub const MAX_IRIS_JSON_BYTES: usize = 1_048_576;
pub const MAX_MESSAGE_NONCE_LEN: usize = 128;
pub const MAX_TX_HASH_LEN: usize = 66;
pub const MAX_SUPPORT_REFERENCE_LEN: usize = 128;
pub const MAX_DECIMAL_AMOUNT_LEN: usize = 64;

pub fn check_byte_len(label: &str, bytes: &[u8], max: usize) -> Result<(), String> {
    if bytes.len() > max {
        return Err(format!("{label} exceeds max {max} bytes"));
    }
    Ok(())
}

pub fn check_str_len(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.len() > max {
        return Err(format!("{label} exceeds max {max} chars"));
    }
    Ok(())
}
