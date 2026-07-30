//! Authoritative Stellar account sequence lookup for CCTP builders.
//!
//! Prefers Soroban RPC `getLedgerEntries` (account ledger key); falls back to configured
//! Horizon when RPC entry lookup fails. Never accepts client-supplied sequence.

use async_trait::async_trait;
use std::sync::Arc;

use crate::cctp::config::CctpConfig;
use crate::cctp::stellar_rpc::StellarRpcClient;
use crate::swap::tx::{AccountSequenceSource, HorizonAccountSequences, TxBuildError};

pub struct RpcAccountSequenceSource {
    rpc: Arc<StellarRpcClient>,
    horizon: HorizonAccountSequences,
}

impl RpcAccountSequenceSource {
    pub fn new(config: &CctpConfig, rpc: Arc<StellarRpcClient>) -> Self {
        let mut horizon_urls = Vec::new();
        if !config.stellar_horizon_url.trim().is_empty() {
            horizon_urls.push(
                config
                    .stellar_horizon_url
                    .trim()
                    .trim_end_matches('/')
                    .to_string(),
            );
        }
        if horizon_urls.is_empty() {
            horizon_urls.push("https://horizon-testnet.stellar.org".to_string());
        }
        let horizon = HorizonAccountSequences::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default(),
            horizon_urls,
        );
        Self { rpc, horizon }
    }
}

#[async_trait]
impl AccountSequenceSource for RpcAccountSequenceSource {
    async fn current_sequence(&self, account_id: &str) -> Result<i64, TxBuildError> {
        match self.rpc.get_account_sequence(account_id).await {
            Ok(seq) => Ok(seq),
            Err(_) => self.horizon.current_sequence(account_id).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::config::CctpConfig;

    #[tokio::test]
    async fn rpc_sequence_falls_back_to_horizon() {
        let mut cfg = CctpConfig::default_testnet();
        cfg.stellar_rpc_url = "http://127.0.0.1:1".into();
        cfg.stellar_horizon_url = "http://127.0.0.1:1".into();
        let rpc = Arc::new(StellarRpcClient::new(&cfg).unwrap());
        let source = RpcAccountSequenceSource::new(&cfg, rpc);
        let err = source
            .current_sequence("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")
            .await
            .unwrap_err();
        assert!(matches!(err, TxBuildError::AccountLookup(_)));
    }
}
