//! Production Stellar Testnet `mint_and_forward` verifier via Soroban RPC `getTransaction`.

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::cctp::config::CctpConfig;
use crate::cctp::evm_mint_verifier::EvmRpcMintVerifier;
use crate::cctp::message::parse_cctp_v2_message;
use crate::cctp::stellar_contract_events::{
    contract_hash, parse_message_received, parse_mint_and_forward,
};
use crate::cctp::stellar_rpc::StellarRpcClient;
use crate::cctp::stellar_tx::{
    ensure_testnet_binding, parse_invoke_envelope, scval_to_bytes, TxStatus,
};
use crate::cctp::verifiers::{
    MintVerifyOutcome, StellarMintVerifier, VerifiedMintFacts, VerifierError,
};
use crate::models::v2_cctp::{PreparedWalletPayload, STELLAR_TESTNET_CHAIN_ID};

pub struct StellarRpcMintVerifier {
    rpc: Arc<StellarRpcClient>,
    forwarder: String,
    message_transmitter: String,
    network_passphrase: String,
    probe_ok: bool,
}

impl StellarRpcMintVerifier {
    pub async fn new(config: &CctpConfig) -> Result<Self, VerifierError> {
        ensure_testnet_binding(config)?;
        if config.stellar_rpc_url.trim().is_empty() {
            return Err(VerifierError::NotReady);
        }
        let rpc = Arc::new(StellarRpcClient::new(config)?);
        let probe_ok = rpc.latest_ledger().await.is_ok();
        Ok(Self {
            rpc,
            forwarder: config.contracts.stellar_cctp_forwarder.clone(),
            message_transmitter: config.contracts.stellar_message_transmitter.clone(),
            network_passphrase: config.stellar_network_passphrase.clone(),
            probe_ok,
        })
    }

    fn hash32(data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    fn payload_hash_from_envelope(&self, envelope_xdr: &str) -> Result<String, VerifierError> {
        let payload = PreparedWalletPayload::StellarXdr {
            network_passphrase: self.network_passphrase.clone(),
            xdr_envelope: envelope_xdr.to_string(),
        };
        let json =
            serde_json::to_string(&payload).map_err(|e| VerifierError::Failed(e.to_string()))?;
        Ok(hex::encode(Sha256::digest(json.as_bytes())))
    }

    fn decode_mint_invoke(
        invoke: &crate::cctp::stellar_tx::ParsedInvoke,
    ) -> Result<(Vec<u8>, Vec<u8>), VerifierError> {
        if invoke.function != "mint_and_forward" {
            return Err(VerifierError::Failed("wrong function".into()));
        }
        if invoke.args.len() != 2 {
            return Err(VerifierError::Failed("mint arg count".into()));
        }
        Ok((
            scval_to_bytes(&invoke.args[0])?,
            scval_to_bytes(&invoke.args[1])?,
        ))
    }

    async fn completion_outcome(
        &self,
        tx_hash: &str,
        message: &[u8],
        nonce: &str,
        recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
        let expected_nonce = EvmRpcMintVerifier::parse_stored_nonce(nonce)?;
        let parsed =
            parse_cctp_v2_message(message).map_err(|e| VerifierError::Failed(e.to_string()))?;
        if parsed.nonce != expected_nonce {
            return Err(VerifierError::Failed("nonce/message mismatch".into()));
        }

        let tx = self.rpc.get_finalized_transaction(tx_hash).await?;
        if tx.status == TxStatus::Failed {
            return Ok(MintVerifyOutcome::FailedRetryable {
                reason: "tx failed".into(),
            });
        }

        let mt_hash = crate::cctp::encoding::stellar_contract_to_bytes32(&self.message_transmitter)
            .map_err(|e| VerifierError::Failed(e.to_string()))?;
        let fwd_hash = crate::cctp::encoding::stellar_contract_to_bytes32(&self.forwarder)
            .map_err(|e| VerifierError::Failed(e.to_string()))?;

        let mut forward_matches = 0usize;
        let mut received_matches = 0usize;

        for event in &tx.contract_events {
            let hash = contract_hash(event)?;
            if hash == fwd_hash {
                if let Ok(ev) = parse_mint_and_forward(event) {
                    if ev.forward_recipient == recipient {
                        forward_matches += 1;
                    }
                }
            }
            if hash == mt_hash {
                if let Ok(ev) = parse_message_received(event) {
                    if ev.nonce == expected_nonce
                        && ev.source_domain == parsed.source_domain
                        && ev.sender == parsed.sender
                    {
                        received_matches += 1;
                    }
                }
            }
        }

        if forward_matches == 1 || received_matches == 1 {
            return Ok(MintVerifyOutcome::Succeeded);
        }
        if forward_matches > 1 || received_matches > 1 {
            return Err(VerifierError::Failed("ambiguous mint events".into()));
        }

        match self
            .rpc
            .simulate_is_nonce_used(&self.message_transmitter, expected_nonce)
            .await
        {
            Ok(true) => Ok(MintVerifyOutcome::NonceUsed),
            Ok(false) => Ok(MintVerifyOutcome::Pending),
            Err(VerifierError::Transient(m)) => Err(VerifierError::Transient(m)),
            Err(VerifierError::NotReady) => Err(VerifierError::NotReady),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl StellarMintVerifier for StellarRpcMintVerifier {
    fn is_ready(&self) -> bool {
        self.probe_ok && self.rpc.is_ready()
    }

    async fn verify_mint_submission(
        &self,
        tx_hash: &str,
        message: &[u8],
        attestation: &[u8],
        nonce: &str,
        expected_payload_hash: &str,
    ) -> Result<VerifiedMintFacts, VerifierError> {
        if !self.is_ready() {
            return Err(VerifierError::NotReady);
        }

        let tx = self.rpc.get_finalized_transaction(tx_hash).await?;
        if tx.status == TxStatus::Failed {
            return Ok(VerifiedMintFacts {
                tx_hash: tx.tx_hash,
                destination_chain_id: STELLAR_TESTNET_CHAIN_ID.into(),
                contract_address: self.forwarder.clone(),
                function_selector: "mint_and_forward".into(),
                message_hash: Self::hash32(message),
                attestation_hash: Self::hash32(attestation),
                nonce: nonce.to_string(),
                payload_hash: expected_payload_hash.to_string(),
                outcome: MintVerifyOutcome::FailedRetryable {
                    reason: "tx failed".into(),
                },
                recipient_evidence: None,
            });
        }

        let invoke = parse_invoke_envelope(&tx.envelope_xdr)?;
        if invoke.contract_strkey != self.forwarder {
            return Err(VerifierError::Failed("wrong contract".into()));
        }
        let (tx_message, tx_attestation) = Self::decode_mint_invoke(&invoke)?;
        if tx_message != message || tx_attestation != attestation {
            return Err(VerifierError::Failed("message/attestation mismatch".into()));
        }

        let computed_hash = self.payload_hash_from_envelope(&tx.envelope_xdr)?;
        if computed_hash != expected_payload_hash {
            return Err(VerifierError::Failed("payload hash mismatch".into()));
        }

        let outcome = if tx.status == TxStatus::Success {
            MintVerifyOutcome::Pending
        } else {
            MintVerifyOutcome::FailedRetryable {
                reason: "tx failed".into(),
            }
        };

        Ok(VerifiedMintFacts {
            tx_hash: tx.tx_hash,
            destination_chain_id: STELLAR_TESTNET_CHAIN_ID.into(),
            contract_address: self.forwarder.clone(),
            function_selector: "mint_and_forward".into(),
            message_hash: Self::hash32(message),
            attestation_hash: Self::hash32(attestation),
            nonce: nonce.to_string(),
            payload_hash: expected_payload_hash.to_string(),
            outcome,
            recipient_evidence: Some(invoke.source_account.clone()),
        })
    }

    async fn verify_mint_completion(
        &self,
        tx_hash: &str,
        message: &[u8],
        nonce: &str,
        recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
        if !self.is_ready() {
            return Err(VerifierError::NotReady);
        }
        self.completion_outcome(tx_hash, message, nonce, recipient)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::builders::stellar::encoder::encode_invoke_at_sequence;
    use crate::cctp::config::CctpConfig;

    #[tokio::test]
    async fn not_ready_without_rpc() {
        let mut cfg = CctpConfig::default_testnet();
        cfg.stellar_rpc_url = String::new();
        assert!(matches!(
            StellarRpcMintVerifier::new(&cfg).await,
            Err(VerifierError::NotReady)
        ));
    }

    #[test]
    fn payload_hash_is_deterministic() {
        let cfg = CctpConfig::default_testnet();
        let verifier = StellarRpcMintVerifier {
            rpc: Arc::new(StellarRpcClient::new(&cfg).unwrap()),
            forwarder: cfg.contracts.stellar_cctp_forwarder.clone(),
            message_transmitter: cfg.contracts.stellar_message_transmitter.clone(),
            network_passphrase: cfg.stellar_network_passphrase.clone(),
            probe_ok: true,
        };
        let xdr = "AAAAAgAAAADuBg+afmvWN9+nlruudR93UO1rDpTe8i6yxgPgBKoBVwAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAABAAAAAA==";
        let h1 = verifier.payload_hash_from_envelope(xdr).unwrap();
        let h2 = verifier.payload_hash_from_envelope(xdr).unwrap();
        assert_eq!(h1, h2);
    }
}
