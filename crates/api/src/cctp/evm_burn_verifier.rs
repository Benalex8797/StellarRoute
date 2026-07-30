//! Production EVM Sepolia burn verifier via JSON-RPC.

use alloy_primitives::{Address, Log, B256};
use alloy_sol_types::{sol, SolEvent};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

use crate::cctp::builders::evm::SEPOLIA_CHAIN_ID_NUM;
use crate::cctp::config::CctpConfig;
use crate::cctp::verifiers::{EvmBurnVerifier, VerifiedBurnFacts, VerifierError};

const MIN_CONFIRMATIONS: u64 = 1;

sol! {
    event DepositForBurn(
        address indexed burnToken,
        uint256 amount,
        address indexed depositor,
        bytes32 mintRecipient,
        uint32 destinationDomain,
        bytes32 destinationTokenMessenger,
        bytes32 destinationCaller,
        uint256 maxFee,
        uint32 indexed minFinalityThreshold,
        bytes hookData
    );
}

pub struct EvmRpcBurnVerifier {
    client: Client,
    rpc_url: String,
    token_messenger: Address,
    chain_id: u64,
}

impl EvmRpcBurnVerifier {
    pub fn new(config: &CctpConfig) -> Result<Self, VerifierError> {
        let token_messenger = config
            .contracts
            .sepolia_token_messenger
            .trim()
            .parse()
            .map_err(|_| VerifierError::Failed("token messenger address".into()))?;
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|e| VerifierError::Failed(e.to_string()))?,
            rpc_url: config.sepolia_rpc_url.clone(),
            token_messenger,
            chain_id: SEPOLIA_CHAIN_ID_NUM,
        })
    }

    fn normalize_hash(hash: &str) -> String {
        let trimmed = hash.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .unwrap_or(trimmed);
        format!("0x{}", hex.to_ascii_lowercase())
    }

    async fn rpc_call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, VerifierError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VerifierError::Failed(e.to_string()))?;
        let payload: RpcResponse<T> = resp
            .json()
            .await
            .map_err(|e| VerifierError::Failed(e.to_string()))?;
        if let Some(err) = payload.error {
            return Err(VerifierError::Failed(err.message));
        }
        payload.result.ok_or(VerifierError::TxNotFound)
    }
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EthTransaction {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
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

#[async_trait]
impl EvmBurnVerifier for EvmRpcBurnVerifier {
    fn is_ready(&self) -> bool {
        true
    }

    async fn verify_burn(&self, tx_hash: &str) -> Result<VerifiedBurnFacts, VerifierError> {
        let hash = Self::normalize_hash(tx_hash);
        let tx: EthTransaction = self
            .rpc_call("eth_getTransactionByHash", json!([hash]))
            .await?;
        let receipt: EthReceipt = self
            .rpc_call("eth_getTransactionReceipt", json!([hash]))
            .await?;

        if receipt.status.as_deref() != Some("0x1") {
            return Err(VerifierError::Failed("tx failed".into()));
        }

        let from = tx
            .from
            .as_deref()
            .ok_or_else(|| VerifierError::Failed("missing from".into()))?;
        let to = tx
            .to
            .as_deref()
            .ok_or_else(|| VerifierError::Failed("missing to".into()))?
            .to_ascii_lowercase();
        if to != format!("{:#x}", self.token_messenger).to_ascii_lowercase()
            && to != format!("0x{:x}", self.token_messenger).to_ascii_lowercase()
        {
            return Err(VerifierError::Failed("wrong contract".into()));
        }

        let chain_id_resp: String = self.rpc_call("eth_chainId", json!([])).await?;
        let parsed_chain = u64::from_str_radix(chain_id_resp.trim_start_matches("0x"), 16)
            .map_err(|_| VerifierError::Failed("chain id parse".into()))?;
        if parsed_chain != self.chain_id {
            return Err(VerifierError::Failed("wrong chain".into()));
        }

        let latest: EthBlockNumber = self.rpc_call("eth_blockNumber", json!([])).await?;
        let latest_num = u64::from_str_radix(latest.0.trim_start_matches("0x"), 16)
            .map_err(|_| VerifierError::Failed("block parse".into()))?;
        let tx_block = receipt
            .block_number
            .as_deref()
            .ok_or_else(|| VerifierError::Failed("pending".into()))?;
        let tx_num = u64::from_str_radix(tx_block.trim_start_matches("0x"), 16)
            .map_err(|_| VerifierError::Failed("tx block parse".into()))?;
        if latest_num.saturating_sub(tx_num) + 1 < MIN_CONFIRMATIONS {
            return Err(VerifierError::Failed("insufficient confirmations".into()));
        }

        let logs = receipt.logs.unwrap_or_default();
        let mut matches = 0usize;
        let mut parsed_event: Option<DepositForBurn> = None;
        for log in logs {
            if !log
                .address
                .eq_ignore_ascii_case(&format!("{:#x}", self.token_messenger))
            {
                continue;
            }
            let topics: Vec<B256> = log.topics.iter().filter_map(|t| t.parse().ok()).collect();
            if topics.is_empty() {
                continue;
            }
            let alloy_log = Log {
                address: self.token_messenger,
                data: alloy_primitives::LogData::new_unchecked(
                    topics,
                    log.data.parse().unwrap_or_default(),
                ),
            };
            if let Ok(decoded) = DepositForBurn::decode_log(&alloy_log, true) {
                matches += 1;
                parsed_event = Some(decoded.data);
            }
        }
        if matches != 1 {
            return Err(VerifierError::Failed("ambiguous burn logs".into()));
        }
        let event = parsed_event.ok_or_else(|| VerifierError::Failed("no burn event".into()))?;

        let hook_data = if event.hookData.is_empty() {
            None
        } else {
            Some(event.hookData.to_vec())
        };

        Ok(VerifiedBurnFacts {
            tx_hash: hash,
            source_chain_id: format!("eip155:{}", self.chain_id),
            source_domain: 0,
            destination_domain: event.destinationDomain,
            sender: from.to_string(),
            amount_cctp_subunits: event.amount.try_into().unwrap_or(0),
            burn_token_bytes32: address_to_bytes32(event.burnToken),
            mint_recipient_bytes32: event.mintRecipient.0,
            destination_caller_bytes32: event.destinationCaller.0,
            min_finality_threshold: event.minFinalityThreshold,
            hook_data,
            token_messenger_bytes32: address_to_bytes32(self.token_messenger),
            block_or_ledger: receipt.block_number,
        })
    }
}

fn address_to_bytes32(addr: Address) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..32].copy_from_slice(addr.as_slice());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn rejects_failed_receipt() {
        let server = MockServer::start().await;
        let mut cfg = CctpConfig::default_testnet();
        cfg.sepolia_rpc_url = server.uri();
        let verifier = EvmRpcBurnVerifier::new(&cfg).unwrap();

        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "from": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0",
                    "to": cfg.contracts.sepolia_token_messenger,
                    "input": "0x"
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": { "status": "0x0", "logs": [] }
            })))
            .mount(&server)
            .await;

        let err = verifier
            .verify_burn("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef")
            .await
            .unwrap_err();
        assert_eq!(err, VerifierError::Failed("tx failed".into()));
    }
}
