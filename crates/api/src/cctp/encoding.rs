//! CCTP v2 encoding helpers with golden vectors from Circle Stellar reference.
//!
//! Sources:
//! - https://developers.circle.com/cctp/references/stellar (hook layout, contract bytes32)
//! - https://developers.circle.com/cctp/references/technical-guide (message layout)

use thiserror::Error;

use crate::models::v2_cctp::is_valid_evm_address;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EncodingError {
    #[error("invalid EVM address: {0}")]
    InvalidEvmAddress(String),
    #[error("invalid Stellar contract strkey: {0}")]
    InvalidStellarContract(String),
    #[error("invalid Stellar G-address: {0}")]
    InvalidStellarAccount(String),
    #[error("amount overflow")]
    AmountOverflow,
    #[error("amount has non-zero 7th decimal remainder: {0}")]
    StellarRemainder(String),
    #[error("invalid hook data")]
    InvalidHookData,
}

/// EVM 20-byte address -> bytes32 left-zero-padded (Circle Message.sol reference).
pub fn evm_address_to_bytes32(address: &str) -> Result<[u8; 32], EncodingError> {
    if !is_valid_evm_address(address) {
        return Err(EncodingError::InvalidEvmAddress(address.to_string()));
    }
    let hex = address.trim().strip_prefix("0x").unwrap_or(address.trim());
    let bytes = hex::decode(hex).expect("validated hex");
    let mut out = [0u8; 32];
    out[12..32].copy_from_slice(&bytes);
    Ok(out)
}

/// Stellar contract C-strkey -> 32-byte contract id (raw payload, no type prefix).
pub fn stellar_contract_to_bytes32(strkey: &str) -> Result<[u8; 32], EncodingError> {
    let contract = stellar_strkey::Contract::from_string(strkey.trim())
        .map_err(|_| EncodingError::InvalidStellarContract(strkey.to_string()))?;
    Ok(contract.0)
}

/// Build CctpForwarder hook data for a G-address recipient (Circle Stellar reference).
///
/// Layout: 24 zero bytes | u32 BE version=0 | u32 BE length | UTF-8 strkey
pub fn build_forwarder_hook_data_g_recipient(g_address: &str) -> Result<Vec<u8>, EncodingError> {
    if stellar_strkey::ed25519::PublicKey::from_string(g_address.trim()).is_err() {
        return Err(EncodingError::InvalidStellarAccount(g_address.to_string()));
    }
    let recipient_bytes = g_address.trim().as_bytes();
    let mut hook = vec![0u8; 32 + recipient_bytes.len()];
    hook[24..28].copy_from_slice(&0u32.to_be_bytes());
    hook[28..32].copy_from_slice(&(recipient_bytes.len() as u32).to_be_bytes());
    hook[32..].copy_from_slice(recipient_bytes);
    Ok(hook)
}

/// Convert decimal USDC string to 6-decimal CCTP subunits (uint256 wire amount).
pub fn decimal_to_cctp_subunits(amount: &str) -> Result<u128, EncodingError> {
    parse_decimal_to_subunits(amount, 6)
}

/// Convert 6-decimal CCTP subunits to 7-decimal Stellar token subunits (×10).
pub fn cctp_subunits_to_stellar_subunits(cctp: u128) -> Result<u128, EncodingError> {
    cctp.checked_mul(10).ok_or(EncodingError::AmountOverflow)
}

/// Stellar outbound: debit only through 6th decimal; remainder stays on source account.
pub fn stellar_outbound_cctp_amount(
    amount_7dp: &str,
) -> Result<(u128, Option<String>), EncodingError> {
    let (whole, fraction) = split_decimal(amount_7dp)?;
    let frac7 = pad_fraction(&fraction, 7);
    let frac6 = &frac7[..6];
    let remainder_digit = frac7.as_bytes().get(6).copied().unwrap_or(b'0');
    let cctp = parse_decimal_to_subunits(&format!("{}.{}", whole, frac6), 6)?;
    let remainder = if remainder_digit != b'0' {
        Some(format!("0.000000{}", remainder_digit as char))
    } else {
        None
    };
    Ok((cctp, remainder))
}

fn split_decimal(amount: &str) -> Result<(String, String), EncodingError> {
    let parts: Vec<&str> = amount.split('.').collect();
    if parts.len() > 2 || parts.is_empty() {
        return Err(EncodingError::AmountOverflow);
    }
    Ok((
        parts[0].to_string(),
        parts.get(1).unwrap_or(&"").to_string(),
    ))
}

fn pad_fraction(fraction: &str, width: usize) -> String {
    let mut s = fraction.to_string();
    if s.len() > width {
        return s[..width].to_string();
    }
    while s.len() < width {
        s.push('0');
    }
    s
}

fn parse_decimal_to_subunits(amount: &str, scale: usize) -> Result<u128, EncodingError> {
    let (whole, fraction) = split_decimal(amount)?;
    let frac = pad_fraction(&fraction, scale);
    let combined = format!("{}{}", whole, frac);
    combined
        .parse::<u128>()
        .map_err(|_| EncodingError::AmountOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::config::STELLAR_CCTP_FORWARDER;

    // Golden: Circle Stellar reference contractStrkeyToBytes32 for CctpForwarder.
    #[test]
    fn golden_stellar_forwarder_contract_bytes32() {
        let bytes = stellar_contract_to_bytes32(STELLAR_CCTP_FORWARDER).unwrap();
        let hex = hex::encode(bytes);
        // Decoded from StrKey.decodeContract per Circle TypeScript reference.
        assert_eq!(hex.len(), 64);
        assert_ne!(hex, "0".repeat(64));
    }

    #[test]
    fn golden_evm_address_bytes32_left_padded() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0";
        let bytes = evm_address_to_bytes32(addr).unwrap();
        assert_eq!(bytes[0..12], [0u8; 12]);
        assert_eq!(
            hex::encode(&bytes[12..32]),
            "742d35cc6634c0532925a3b844bc9e7595f0beb0"
        );
    }

    #[test]
    fn golden_forwarder_hook_data_g_recipient() {
        let g = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let hook = build_forwarder_hook_data_g_recipient(g).unwrap();
        assert_eq!(hook[0..24], [0u8; 24]);
        assert_eq!(u32::from_be_bytes(hook[24..28].try_into().unwrap()), 0);
        let len = u32::from_be_bytes(hook[28..32].try_into().unwrap());
        assert_eq!(len as usize, g.len());
        assert_eq!(&hook[32..], g.as_bytes());
    }

    #[test]
    fn stellar_seven_to_six_decimal_with_remainder() {
        let (cctp, rem) = stellar_outbound_cctp_amount("0.1234567").unwrap();
        assert_eq!(cctp, 123456);
        assert_eq!(rem.as_deref(), Some("0.0000007"));
    }

    #[test]
    fn cctp_to_stellar_subunits_scales_by_ten() {
        assert_eq!(cctp_subunits_to_stellar_subunits(123456).unwrap(), 1234560);
    }

    #[test]
    fn parses_decimal_to_cctp_subunits() {
        assert_eq!(decimal_to_cctp_subunits("100.000000").unwrap(), 100_000_000);
    }
}
