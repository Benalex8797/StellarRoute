//! Shared Stellar envelope payload hashing — builder and verifier must agree.

use sha2::{Digest, Sha256};
use stellar_xdr::curr::{
    Hash, Limited, Limits, ReadXdr, TransactionEnvelope, TransactionSignaturePayload,
    TransactionSignaturePayloadTaggedTransaction, WriteXdr,
};

use crate::cctp::config::{CctpConfig, STELLAR_TESTNET_PASSPHRASE};
use crate::cctp::verifiers::VerifierError;
use crate::models::v2_cctp::PreparedWalletPayload;

/// Canonical network passphrase for payload hash and tx-id binding.
pub fn passphrase_for_config(config: &CctpConfig) -> String {
    if config.stellar_network_passphrase.is_empty() {
        STELLAR_TESTNET_PASSPHRASE.to_string()
    } else {
        config.stellar_network_passphrase.clone()
    }
}

/// SHA256(JSON `PreparedWalletPayload::StellarXdr`) — matches builder mint/burn payloads.
pub fn payload_hash_from_envelope_xdr(
    envelope_xdr: &str,
    config: &CctpConfig,
) -> Result<String, VerifierError> {
    let payload = PreparedWalletPayload::StellarXdr {
        network_passphrase: passphrase_for_config(config),
        xdr_envelope: envelope_xdr.to_string(),
    };
    let json = serde_json::to_string(&payload).map_err(|e| VerifierError::Failed(e.to_string()))?;
    Ok(hex::encode(Sha256::digest(json.as_bytes())))
}

/// Stellar transaction hash from signed envelope XDR + network passphrase (protocol v1).
pub fn transaction_hash_from_envelope_xdr(
    envelope_xdr: &str,
    network_passphrase: &str,
) -> Result<String, VerifierError> {
    let env = TransactionEnvelope::from_xdr_base64(envelope_xdr, Limits::none())
        .map_err(|e| VerifierError::Failed(e.to_string()))?;
    let payload = match env {
        TransactionEnvelope::Tx(v1) => TransactionSignaturePayload {
            network_id: Hash(Sha256::digest(network_passphrase.as_bytes()).into()),
            tagged_transaction: TransactionSignaturePayloadTaggedTransaction::Tx(v1.tx),
        },
        TransactionEnvelope::TxFeeBump(_) => {
            return Err(VerifierError::Failed(
                "fee-bump envelopes unsupported".into(),
            ));
        }
        TransactionEnvelope::TxV0(_) => {
            return Err(VerifierError::Failed("v0 envelopes unsupported".into()));
        }
    };
    let mut bytes = Vec::new();
    let mut writer = Limited::new(&mut bytes, Limits::none());
    payload
        .write_xdr(&mut writer)
        .map_err(|e| VerifierError::Failed(e.to_string()))?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}
