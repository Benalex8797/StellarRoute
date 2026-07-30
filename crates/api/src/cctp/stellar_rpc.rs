//! Minimal Soroban JSON-RPC client for read-only contract simulation.

use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use stellar_xdr::curr::{ReadXdr, ScVal};

use crate::cctp::builders::stellar::encoder::encode_invoke_at_sequence;
use crate::cctp::config::CctpConfig;
use crate::cctp::verifiers::VerifierError;

pub const MAX_JSON_BODY_BYTES: usize = 256 * 1024;
pub const SIMULATE_SOURCE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

#[derive(Debug, Clone)]
pub struct StellarRpcClient {
    pub client: Client,
    pub rpc_url: String,
    pub network_passphrase: String,
}

impl StellarRpcClient {
    pub fn new(config: &CctpConfig) -> Result<Self, VerifierError> {
        if config.stellar_rpc_url.trim().is_empty() {
            return Err(VerifierError::NotReady);
        }
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|e| VerifierError::Transient(e.to_string()))?,
            rpc_url: config.stellar_rpc_url.clone(),
            network_passphrase: config.stellar_network_passphrase.clone(),
        })
    }

    pub fn is_ready(&self) -> bool {
        !self.rpc_url.trim().is_empty()
    }

    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, VerifierError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let body_str = body.to_string();
        if body_str.len() > MAX_JSON_BODY_BYTES {
            return Err(VerifierError::Failed("rpc request too large".into()));
        }
        let resp = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VerifierError::Transient(e.to_string()))?;
        let text = resp
            .text()
            .await
            .map_err(|e| VerifierError::Transient(e.to_string()))?;
        if text.len() > MAX_JSON_BODY_BYTES {
            return Err(VerifierError::Failed("rpc response too large".into()));
        }
        #[derive(Deserialize)]
        struct RpcResponse<T> {
            result: Option<T>,
            error: Option<RpcError>,
        }
        #[derive(Deserialize)]
        struct RpcError {
            message: String,
        }
        let payload: RpcResponse<T> =
            serde_json::from_str(&text).map_err(|e| VerifierError::Failed(e.to_string()))?;
        if let Some(err) = payload.error {
            return Err(VerifierError::Failed(err.message));
        }
        payload.result.ok_or(VerifierError::TxNotFound)
    }

    pub async fn latest_ledger(&self) -> Result<u32, VerifierError> {
        #[derive(Deserialize)]
        struct LatestLedger {
            sequence: u32,
        }
        let result: LatestLedger = self.call("getLatestLedger", json!({})).await?;
        Ok(result.sequence)
    }

    pub async fn simulate_scval(
        &self,
        contract: &str,
        function: &str,
        args: Vec<stellar_xdr::curr::ScVal>,
    ) -> Result<ScVal, VerifierError> {
        let ledger = self.latest_ledger().await?;
        let xdr =
            encode_invoke_at_sequence(SIMULATE_SOURCE, contract, function, args, ledger as i64)
                .map_err(|e| VerifierError::Failed(e.to_string()))?;
        #[derive(Deserialize)]
        struct SimulateResult {
            results: Vec<SimItem>,
        }
        #[derive(Deserialize)]
        struct SimItem {
            xdr: String,
        }
        let result: SimulateResult = self
            .call(
                "simulateTransaction",
                json!({
                    "transaction": xdr,
                    "resourceConfig": { "instructionLeeway": 1_000_000 }
                }),
            )
            .await?;
        let item = result
            .results
            .first()
            .ok_or(VerifierError::Failed("no sim result".into()))?;
        let scval = ScVal::from_xdr_base64(&item.xdr, stellar_xdr::curr::Limits::none())
            .map_err(|e| VerifierError::Failed(e.to_string()))?;
        Ok(scval)
    }
}

pub fn scval_to_u32(val: &ScVal) -> Result<u32, VerifierError> {
    match val {
        ScVal::U32(v) => Ok(*v),
        ScVal::Void => Err(VerifierError::Failed("option none".into())),
        _ => Err(VerifierError::Failed("expected u32".into())),
    }
}

pub fn scval_to_bool(val: &ScVal) -> Result<bool, VerifierError> {
    match val {
        ScVal::Bool(v) => Ok(*v),
        _ => Err(VerifierError::Failed("expected bool".into())),
    }
}

pub fn bytes20_scval(bytes: [u8; 20]) -> stellar_xdr::curr::ScVal {
    use stellar_xdr::curr::ScBytes;
    stellar_xdr::curr::ScVal::Bytes(ScBytes(
        bytes
            .to_vec()
            .try_into()
            .unwrap_or_else(|_| panic!("bytes20")),
    ))
}
