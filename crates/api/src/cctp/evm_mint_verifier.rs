//! Production Sepolia MessageTransmitterV2 mint verifier.

use alloy_primitives::{Address, Log, B256};
use alloy_sol_types::{sol, SolCall, SolEvent};
use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::cctp::builders::evm::ProductionEvmCctpBuilder;
use crate::cctp::config::CctpConfig;
use crate::cctp::evm_rpc::EvmRpcClient;
use crate::cctp::verifiers::{
    EvmMintVerifier, MintVerifyOutcome, VerifiedMintFacts, VerifierError,
};
use crate::models::v2_cctp::SEPOLIA_CHAIN_ID;

/// `receiveMessage(bytes,bytes)` — MessageTransmitterV2.
/// Cross-check: `cast sig "receiveMessage(bytes,bytes)"` => 0x57ecfd28
pub const RECEIVE_MESSAGE_SELECTOR: [u8; 4] = [0x57, 0xec, 0xfd, 0x28];

const DEFAULT_MIN_CONFIRMATIONS: u64 = 1;

sol! {
    interface IMessageTransmitterV2 {
        function receiveMessage(bytes message, bytes attestation) external returns (bool);
    }

    event MessageReceived(address indexed caller, bytes32 sourceDomain, uint64 indexed nonce);
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EthTransaction {
    from: Option<String>,
    to: Option<String>,
    input: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EthReceipt {
    status: Option<String>,
    logs: Option<Vec<EthLog>>,
    block_number: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EthLog {
    address: String,
    topics: Vec<String>,
    data: String,
}

#[derive(Debug, Deserialize)]
struct EthBlockNumber(String);

pub struct EvmRpcMintVerifier {
    rpc: EvmRpcClient,
    message_transmitter: Address,
    min_confirmations: u64,
}

impl EvmRpcMintVerifier {
    pub fn new(config: &CctpConfig) -> Result<Self, VerifierError> {
        Self::with_confirmations(config, DEFAULT_MIN_CONFIRMATIONS)
    }

    pub fn with_confirmations(
        config: &CctpConfig,
        min_confirmations: u64,
    ) -> Result<Self, VerifierError> {
        let rpc = EvmRpcClient::new(&config.sepolia_rpc_url)?;
        let message_transmitter = config
            .contracts
            .sepolia_message_transmitter
            .trim()
            .parse()
            .map_err(|_| VerifierError::Failed("message transmitter address".into()))?;
        Ok(Self {
            rpc,
            message_transmitter,
            min_confirmations,
        })
    }

    fn hash32(data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    fn decode_receive_input(input: &str) -> Result<(Vec<u8>, Vec<u8>), VerifierError> {
        let bytes = hex::decode(input.trim_start_matches("0x"))
            .map_err(|_| VerifierError::Failed("calldata hex".into()))?;
        let call = IMessageTransmitterV2::receiveMessageCall::abi_decode(&bytes, true)
            .map_err(|e| VerifierError::Failed(e.to_string()))?;
        Ok((call.message.to_vec(), call.attestation.to_vec()))
    }
}

#[async_trait]
impl EvmMintVerifier for EvmRpcMintVerifier {
    fn is_ready(&self) -> bool {
        self.rpc.is_ready()
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
        self.rpc.ensure_chain().await?;
        let hash = EvmRpcClient::normalize_hash(tx_hash);
        let tx: EthTransaction = self
            .rpc
            .call("eth_getTransactionByHash", serde_json::json!([hash]))
            .await?;
        let receipt: EthReceipt = self
            .rpc
            .call("eth_getTransactionReceipt", serde_json::json!([hash]))
            .await?;

        if receipt.status.as_deref() != Some("0x1") {
            return Err(VerifierError::Failed("tx failed".into()));
        }

        let to = tx
            .to
            .as_deref()
            .ok_or_else(|| VerifierError::Failed("missing to".into()))?
            .to_ascii_lowercase();
        if to != format!("{:#x}", self.message_transmitter).to_ascii_lowercase() {
            return Err(VerifierError::Failed("wrong contract".into()));
        }

        let input = tx
            .input
            .as_deref()
            .ok_or_else(|| VerifierError::Failed("missing input".into()))?;
        let (tx_message, tx_attestation) = Self::decode_receive_input(input)?;
        if tx_message != message || tx_attestation != attestation {
            return Err(VerifierError::Failed("message/attestation mismatch".into()));
        }

        let payload = ProductionEvmCctpBuilder::encode_receive_message(message, attestation);
        let payload_wallet = crate::models::v2_cctp::PreparedWalletPayload::EvmTransaction {
            chain_id: SEPOLIA_CHAIN_ID.into(),
            to: format!("{:#x}", self.message_transmitter),
            data: format!("0x{}", hex::encode(&payload)),
            value: "0".into(),
        };
        let computed_hash = crate::cctp::builders::evm::hash_payload(&payload_wallet);
        if computed_hash != expected_payload_hash {
            return Err(VerifierError::Failed("payload hash mismatch".into()));
        }

        let outcome = self
            .verify_mint_completion(tx_hash, nonce, "")
            .await
            .unwrap_or(MintVerifyOutcome::Pending);

        Ok(VerifiedMintFacts {
            tx_hash: hash,
            destination_chain_id: SEPOLIA_CHAIN_ID.into(),
            contract_address: format!("{:#x}", self.message_transmitter),
            function_selector: hex::encode(RECEIVE_MESSAGE_SELECTOR),
            message_hash: Self::hash32(message),
            attestation_hash: Self::hash32(attestation),
            nonce: nonce.to_string(),
            payload_hash: expected_payload_hash.to_string(),
            outcome,
            recipient_evidence: tx.from.clone(),
        })
    }

    async fn verify_mint_completion(
        &self,
        tx_hash: &str,
        nonce: &str,
        _recipient: &str,
    ) -> Result<MintVerifyOutcome, VerifierError> {
        if !self.is_ready() {
            return Err(VerifierError::NotReady);
        }
        let hash = EvmRpcClient::normalize_hash(tx_hash);
        let receipt: EthReceipt = self
            .rpc
            .call("eth_getTransactionReceipt", serde_json::json!([hash]))
            .await?;

        if receipt.status.as_deref() != Some("0x1") {
            return Ok(MintVerifyOutcome::FailedRetryable {
                reason: "tx failed".into(),
            });
        }

        let latest: EthBlockNumber = self
            .rpc
            .call("eth_blockNumber", serde_json::json!([]))
            .await?;
        let latest_num = u64::from_str_radix(latest.0.trim_start_matches("0x"), 16)
            .map_err(|_| VerifierError::Failed("block parse".into()))?;
        let tx_block = receipt
            .block_number
            .as_deref()
            .ok_or_else(|| VerifierError::Failed("pending".into()))?;
        let tx_num = u64::from_str_radix(tx_block.trim_start_matches("0x"), 16)
            .map_err(|_| VerifierError::Failed("tx block parse".into()))?;
        if latest_num.saturating_sub(tx_num) + 1 < self.min_confirmations {
            return Ok(MintVerifyOutcome::Pending);
        }

        let nonce_u64: u64 = nonce
            .parse()
            .map_err(|_| VerifierError::Failed("nonce".into()))?;
        let logs = receipt.logs.unwrap_or_default();
        let mut message_received = 0usize;
        for log in logs {
            if !log
                .address
                .eq_ignore_ascii_case(&format!("{:#x}", self.message_transmitter))
            {
                continue;
            }
            let topics: Vec<B256> = log.topics.iter().filter_map(|t| t.parse().ok()).collect();
            let alloy_log = Log {
                address: self.message_transmitter,
                data: alloy_primitives::LogData::new_unchecked(
                    topics,
                    log.data.parse().unwrap_or_default(),
                ),
            };
            if let Ok(decoded) = MessageReceived::decode_log(&alloy_log, true) {
                if decoded.data.nonce == nonce_u64 {
                    message_received += 1;
                }
            }
        }
        if message_received == 1 {
            return Ok(MintVerifyOutcome::Succeeded);
        }
        if message_received > 1 {
            return Err(VerifierError::Failed("ambiguous mint logs".into()));
        }
        Ok(MintVerifyOutcome::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receive_message_selector_matches_alloy() {
        assert_eq!(
            RECEIVE_MESSAGE_SELECTOR,
            IMessageTransmitterV2::receiveMessageCall::SELECTOR
        );
    }
}
