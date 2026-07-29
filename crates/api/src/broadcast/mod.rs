//! Transaction broadcast abstraction for swap submission.
//!
//! Production code posts signed envelopes to Horizon. Tests inject a mock
//! implementation via [`AppState::transaction_broadcaster`].

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

/// Result of a successful Horizon submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastResult {
    pub tx_hash: String,
    /// `"pending"` when Horizon accepted the tx; `"success"` when included.
    pub status: String,
    pub ledger: Option<u64>,
}

/// Broadcast failure taxonomy mapped to swap submit metrics / audit classes.
#[derive(Debug, Error)]
pub enum BroadcastError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("timeout")]
    Timeout,
    #[error("rpc error: {0}")]
    RpcError(String),
    #[error("insufficient fee")]
    InsufficientFee,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("slippage exceeded")]
    SlippageExceeded,
    #[error("bad signature")]
    BadSignature,
}

impl BroadcastError {
    pub fn metrics_class(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::Timeout => "timeout",
            Self::RpcError(_) => "rpc_error",
            Self::InsufficientFee => "insufficient_fee",
            Self::InsufficientBalance => "insufficient_balance",
            Self::SlippageExceeded => "slippage_exceeded",
            Self::BadSignature => "bad_signature",
        }
    }
}

/// Abstraction for broadcasting signed transaction envelopes.
#[async_trait]
pub trait TransactionBroadcaster: Send + Sync {
    async fn submit(&self, signed_xdr: &str) -> Result<BroadcastResult, BroadcastError>;
}

/// Horizon `POST /transactions` broadcaster.
#[derive(Clone)]
pub struct HorizonTransactionBroadcaster {
    client: Client,
    horizon_urls: Vec<String>,
}

impl HorizonTransactionBroadcaster {
    pub fn new(client: Client, horizon_urls: Vec<String>) -> Self {
        Self {
            client,
            horizon_urls,
        }
    }

    pub fn from_env() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let mut urls: Vec<String> = std::env::var("STELLAR_HORIZON_URL")
            .ok()
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .into_iter()
            .filter(|u| !u.is_empty())
            .collect();

        if let Ok(extra) = std::env::var("STELLAR_HORIZON_FALLBACK_URLS") {
            for u in extra.split(',') {
                let u = u.trim().trim_end_matches('/').to_string();
                if !u.is_empty() {
                    urls.push(u);
                }
            }
        }

        if urls.is_empty() {
            urls.push("https://horizon-testnet.stellar.org".to_string());
        }

        Self::new(client, urls)
    }
}

#[derive(Debug, Deserialize)]
struct HorizonTxResponse {
    hash: String,
    #[serde(default)]
    successful: Option<bool>,
    ledger: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct HorizonErrorBody {
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    extras: Option<HorizonErrorExtras>,
}

#[derive(Debug, Deserialize)]
struct HorizonErrorExtras {
    #[serde(default)]
    result_codes: Option<HorizonResultCodes>,
}

#[derive(Debug, Deserialize)]
struct HorizonResultCodes {
    #[serde(default)]
    transaction: Option<String>,
    #[serde(default)]
    operations: Option<Vec<String>>,
}

#[async_trait]
impl TransactionBroadcaster for HorizonTransactionBroadcaster {
    async fn submit(&self, signed_xdr: &str) -> Result<BroadcastResult, BroadcastError> {
        if signed_xdr.trim().is_empty() {
            return Err(BroadcastError::Validation(
                "signed_xdr must be non-empty".to_string(),
            ));
        }

        let body = format!("tx={}", urlencoding::encode(signed_xdr.trim()));
        let mut last_err = BroadcastError::RpcError("no horizon URLs configured".to_string());

        for base in &self.horizon_urls {
            let url = format!("{base}/transactions");
            let response = match self
                .client
                .post(&url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body.clone())
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        last_err = BroadcastError::Timeout;
                    } else {
                        last_err = BroadcastError::RpcError(e.to_string());
                    }
                    continue;
                }
            };

            if response.status().is_success() {
                let parsed: HorizonTxResponse = response.json().await.map_err(|e| {
                    BroadcastError::RpcError(format!("invalid horizon response: {e}"))
                })?;
                let status = if parsed.successful == Some(true) {
                    "success".to_string()
                } else {
                    "pending".to_string()
                };
                return Ok(BroadcastResult {
                    tx_hash: parsed.hash,
                    status,
                    ledger: parsed.ledger,
                });
            }

            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if let Ok(err_body) = serde_json::from_str::<HorizonErrorBody>(&text) {
                if let Some(codes) = err_body.extras.and_then(|e| e.result_codes) {
                    if codes.transaction.as_deref() == Some("tx_insufficient_fee") {
                        return Err(BroadcastError::InsufficientFee);
                    }
                    if codes
                        .operations
                        .as_ref()
                        .is_some_and(|ops| ops.iter().any(|c| c.contains("underfunded")))
                    {
                        return Err(BroadcastError::InsufficientBalance);
                    }
                    if codes
                        .operations
                        .as_ref()
                        .is_some_and(|ops| ops.iter().any(|c| c.contains("slippage")))
                    {
                        return Err(BroadcastError::SlippageExceeded);
                    }
                    if codes.transaction.as_deref() == Some("tx_bad_auth") {
                        return Err(BroadcastError::BadSignature);
                    }
                }
                let msg = err_body
                    .detail
                    .or(err_body.title)
                    .unwrap_or_else(|| format!("HTTP {status}"));
                last_err = BroadcastError::RpcError(msg);
            } else {
                last_err = BroadcastError::RpcError(format!("HTTP {status}: {text}"));
            }

            if status.is_client_error() {
                return Err(last_err);
            }
        }

        Err(last_err)
    }
}

/// In-memory mock for unit/integration tests.
#[derive(Default)]
pub struct MockTransactionBroadcaster {
    pub result: std::sync::Mutex<Option<Result<BroadcastResult, BroadcastError>>>,
    pub calls: std::sync::Mutex<Vec<String>>,
}

impl MockTransactionBroadcaster {
    pub fn succeed(tx_hash: impl Into<String>) -> Self {
        Self {
            result: std::sync::Mutex::new(Some(Ok(BroadcastResult {
                tx_hash: tx_hash.into(),
                status: "pending".to_string(),
                ledger: None,
            }))),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn fail(err: BroadcastError) -> Self {
        Self {
            result: std::sync::Mutex::new(Some(Err(err))),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl TransactionBroadcaster for MockTransactionBroadcaster {
    async fn submit(&self, signed_xdr: &str) -> Result<BroadcastResult, BroadcastError> {
        self.calls.lock().unwrap().push(signed_xdr.to_string());
        let mut guard = self.result.lock().unwrap();
        if let Some(result) = guard.take() {
            return result;
        }
        Ok(BroadcastResult {
            tx_hash: "mock-tx-hash".to_string(),
            status: "pending".to_string(),
            ledger: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_error_metrics_classes_are_stable() {
        assert_eq!(
            BroadcastError::BadSignature.metrics_class(),
            "bad_signature"
        );
        assert_eq!(BroadcastError::Timeout.metrics_class(), "timeout");
    }
}
