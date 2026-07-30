//! Production Stellar Testnet SEP-41 approval verifier via Soroban RPC `getTransaction`.

use async_trait::async_trait;
use std::sync::Arc;

use crate::cctp::approval::{StellarApprovalVerifier, VerifiedApprovalFacts};
use crate::cctp::config::CctpConfig;
use crate::cctp::stellar_contract_events::address_to_strkey;
use crate::cctp::stellar_rpc::StellarRpcClient;
use crate::cctp::stellar_tx::{
    ensure_testnet_binding, parse_invoke_envelope, scval_to_address, scval_to_i128, TxStatus,
};
use crate::cctp::store::CctpTransfer;
use crate::cctp::verifiers::VerifierError;
use crate::models::v2_cctp::STELLAR_TESTNET_CHAIN_ID;

pub struct StellarRpcApprovalVerifier {
    rpc: Arc<StellarRpcClient>,
    usdc: String,
    token_messenger: String,
    probe_ok: bool,
}

impl StellarRpcApprovalVerifier {
    pub async fn new(config: &CctpConfig) -> Result<Self, VerifierError> {
        ensure_testnet_binding(config)?;
        if config.stellar_rpc_url.trim().is_empty() {
            return Err(VerifierError::NotReady);
        }
        let rpc = Arc::new(StellarRpcClient::new(config)?);
        let probe_ok = rpc.latest_ledger().await.is_ok();
        Ok(Self {
            rpc,
            usdc: config.contracts.stellar_usdc.clone(),
            token_messenger: config.contracts.stellar_token_messenger.clone(),
            probe_ok,
        })
    }

    fn decode_approve(
        invoke: &crate::cctp::stellar_tx::ParsedInvoke,
    ) -> Result<(String, i128), VerifierError> {
        if invoke.function != "approve" {
            return Err(VerifierError::Failed("wrong function".into()));
        }
        if invoke.args.len() != 2 && invoke.args.len() != 3 {
            return Err(VerifierError::Failed("approve arg count".into()));
        }
        let spender = address_to_strkey(&scval_to_address(&invoke.args[0])?)?;
        let amount = scval_to_i128(&invoke.args[1])?;
        Ok((spender, amount))
    }
}

#[async_trait]
impl StellarApprovalVerifier for StellarRpcApprovalVerifier {
    fn is_ready(&self) -> bool {
        self.probe_ok && self.rpc.is_ready()
    }

    async fn verify_approval(
        &self,
        transfer: &CctpTransfer,
        tx_hash: &str,
        required_amount: i128,
    ) -> Result<VerifiedApprovalFacts, VerifierError> {
        if !self.is_ready() {
            return Err(VerifierError::NotReady);
        }
        let tx = self.rpc.get_finalized_transaction(tx_hash).await?;
        if tx.status != TxStatus::Success {
            return Err(VerifierError::Failed("tx failed".into()));
        }

        let invoke = parse_invoke_envelope(&tx.envelope_xdr)?;
        if !invoke.source_account.eq_ignore_ascii_case(&transfer.sender) {
            return Err(VerifierError::Failed("wrong sender".into()));
        }
        if invoke.contract_strkey != self.usdc {
            return Err(VerifierError::Failed("wrong token contract".into()));
        }

        let (spender, approved) = Self::decode_approve(&invoke)?;
        if spender != self.token_messenger {
            return Err(VerifierError::Failed("wrong spender".into()));
        }
        if approved < required_amount {
            return Err(VerifierError::Failed("insufficient approval amount".into()));
        }

        Ok(VerifiedApprovalFacts {
            tx_hash: tx.tx_hash,
            owner: transfer.sender.clone(),
            token_contract: self.usdc.clone(),
            spender_contract: self.token_messenger.clone(),
            amount: approved as u128,
            chain_id: STELLAR_TESTNET_CHAIN_ID.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::builders::stellar::encoder::{
        approve_args, contract_address, encode_invoke_at_sequence,
    };
    use crate::cctp::config::CctpConfig;
    use crate::cctp::stellar_contract_events::test_helpers::event_to_b64;
    use crate::cctp::stellar_tx::normalize_stellar_tx_hash;
    use crate::cctp::store::CctpTransfer;
    use crate::models::v2_cctp::{
        CctpDirection, CctpFinality, CctpTransferStatus, SEPOLIA_CHAIN_ID, STELLAR_TESTNET_CHAIN_ID,
    };
    use chrono::Utc;
    use uuid::Uuid;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_transfer() -> CctpTransfer {
        let now = Utc::now();
        CctpTransfer {
            transfer_id: Uuid::new_v4(),
            support_reference_id: "s".into(),
            corridor_id: "c".into(),
            provider: "p".into(),
            direction: CctpDirection::StellarToEvm,
            source_chain_id: STELLAR_TESTNET_CHAIN_ID.into(),
            destination_chain_id: SEPOLIA_CHAIN_ID.into(),
            source_asset: "a".into(),
            source_asset_canonical: "a".into(),
            destination_asset: "b".into(),
            destination_asset_canonical: "b".into(),
            sender: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".into(),
            recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
            amount: "1.0000000".into(),
            destination_amount: "1".into(),
            finality: CctpFinality::Standard,
            runtime_fee_quote: None,
            max_fee: None,
            fee_expires_at: None,
            quote_expires_at: now + chrono::Duration::minutes(5),
            status: CctpTransferStatus::BurnPrepared,
            source_tx_hash: None,
            source_approval_tx_hash: None,
            source_approval_verified_at: None,
            destination_tx_hash: None,
            iris_message_hash: None,
            message_nonce: None,
            raw_message: None,
            attestation: None,
            retry_count: 0,
            last_provider_error: None,
            last_provider_code: None,
            version: 1,
            created_at: now,
            updated_at: now,
            terminal_at: None,
            mint_payload_hash: None,
            mint_payload_expires_at: None,
        }
    }

    #[tokio::test]
    async fn rejects_failed_tx() {
        let server = MockServer::start().await;
        let mut cfg = CctpConfig::default_testnet();
        cfg.stellar_rpc_url = server.uri();
        let hash = "a".repeat(64);

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("getLatestLedger"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "result": { "sequence": 100 }
            })))
            .expect(2)
            .mount(&server)
            .await;

        let verifier = StellarRpcApprovalVerifier::new(&cfg).await.unwrap();

        Mock::given(method("POST"))
            .and(body_string_contains("getTransaction"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": {
                    "status": "FAILED",
                    "txHash": hash,
                    "ledger": 99,
                    "envelopeXdr": "AAAAAgAAAADuBg+afmvWN9+nlruudR93UO1rDpTe8i6yxgPgBKoBVwAAAGQAAAAAAAAAAQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAABAAAAAA==",
                    "events": { "contractEventsXdr": [] }
                }
            })))
            .mount(&server)
            .await;

        let err = verifier
            .verify_approval(&sample_transfer(), &hash, 1)
            .await
            .unwrap_err();
        assert_eq!(err, VerifierError::Failed("tx failed".into()));
    }

    #[tokio::test]
    async fn accepts_synthetic_approve_invoke() {
        let server = MockServer::start().await;
        let mut cfg = CctpConfig::default_testnet();
        cfg.stellar_rpc_url = server.uri();
        let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let xdr = encode_invoke_at_sequence(
            source,
            &cfg.contracts.stellar_usdc,
            "approve",
            approve_args(&cfg.contracts.stellar_token_messenger, 10_000_000).unwrap(),
            100,
        )
        .unwrap();
        let hash = "b".repeat(64);

        Mock::given(method("POST"))
            .and(body_string_contains("getLatestLedger"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "result": { "sequence": 102 }
            })))
            .expect(2)
            .mount(&server)
            .await;

        let verifier = StellarRpcApprovalVerifier::new(&cfg).await.unwrap();

        Mock::given(method("POST"))
            .and(body_string_contains("getTransaction"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": {
                    "status": "SUCCESS",
                    "txHash": hash,
                    "ledger": 100,
                    "envelopeXdr": xdr,
                    "events": { "contractEventsXdr": [[]] }
                }
            })))
            .mount(&server)
            .await;

        let facts = verifier
            .verify_approval(&sample_transfer(), &hash, 5_000_000)
            .await
            .unwrap();
        assert_eq!(facts.amount, 10_000_000);
        assert_eq!(normalize_stellar_tx_hash(&facts.tx_hash).unwrap(), hash);
    }
}
