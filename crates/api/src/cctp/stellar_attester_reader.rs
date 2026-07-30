//! Stellar Testnet MessageTransmitterV2 on-chain attester-set reader.

use std::sync::Arc;

use async_trait::async_trait;

use crate::cctp::attester_set::{
    AttesterDestination, AttesterSetError, AttesterSetReader, AttesterSetSnapshot,
};
use crate::cctp::config::CctpConfig;
use crate::cctp::stellar_rpc::{bytes20_scval, scval_to_bool, scval_to_u32, StellarRpcClient};

pub struct StellarAttesterSetReader {
    rpc: StellarRpcClient,
    contract: String,
}

impl StellarAttesterSetReader {
    pub fn new(config: &CctpConfig) -> Result<Self, AttesterSetError> {
        if config.stellar_rpc_url.trim().is_empty() {
            return Err(AttesterSetError::NotReady);
        }
        let rpc = StellarRpcClient::new(config)
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        Ok(Self {
            rpc,
            contract: config.contracts.stellar_message_transmitter.clone(),
        })
    }
}

#[async_trait]
impl AttesterSetReader for StellarAttesterSetReader {
    fn destination(&self) -> AttesterDestination {
        AttesterDestination::StellarTestnet
    }

    async fn read_snapshot(
        &self,
        iris_candidates: &[[u8; 20]],
        iris_set_hash: [u8; 32],
    ) -> Result<AttesterSetSnapshot, AttesterSetError> {
        let ledger = self
            .rpc
            .latest_ledger()
            .await
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;

        let threshold_val = self
            .rpc
            .simulate_scval(&self.contract, "get_signature_threshold", vec![])
            .await
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        let threshold =
            scval_to_u32(&threshold_val).map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        if threshold == 0 {
            return Err(AttesterSetError::ThresholdZero);
        }

        let mut enabled = Vec::new();
        for candidate in iris_candidates {
            let val = self
                .rpc
                .simulate_scval(
                    &self.contract,
                    "is_enabled_attester",
                    vec![bytes20_scval(*candidate)],
                )
                .await
                .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
            if scval_to_bool(&val).map_err(|e| AttesterSetError::Transient(e.to_string()))? {
                enabled.push(*candidate);
            }
        }
        enabled.sort();
        enabled.dedup();
        if (enabled.len() as u32) < threshold {
            return Err(AttesterSetError::InsufficientEnabled);
        }

        Ok(AttesterSetSnapshot {
            destination: AttesterDestination::StellarTestnet,
            signature_threshold: threshold,
            enabled_addresses: enabled,
            iris_set_hash,
            verified_at: std::time::Instant::now(),
            block_or_ledger: Some(ledger.to_string()),
            source: "stellar_message_transmitter_v2",
        })
    }
}

pub fn stellar_reader_arc(
    config: &CctpConfig,
) -> Result<Arc<dyn AttesterSetReader>, AttesterSetError> {
    Ok(Arc::new(StellarAttesterSetReader::new(config)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_rpc() {
        let mut cfg = CctpConfig::default_testnet();
        cfg.stellar_rpc_url.clear();
        assert!(matches!(
            StellarAttesterSetReader::new(&cfg),
            Err(AttesterSetError::NotReady)
        ));
    }
}
