//! Sepolia MessageTransmitterV2 on-chain attester-set reader.

use std::sync::Arc;

use alloy_primitives::Address;
use alloy_sol_types::{sol, SolCall};
use async_trait::async_trait;

use crate::cctp::attester_set::{
    AttesterDestination, AttesterSetError, AttesterSetReader, AttesterSetSnapshot,
};
use crate::cctp::builders::evm::SEPOLIA_CHAIN_ID_NUM;
use crate::cctp::config::CctpConfig;
use crate::cctp::evm_rpc::EvmRpcClient;

sol! {
    interface IMessageTransmitterAttestable {
        function signatureThreshold() external view returns (uint256);
        function isEnabledAttester(address attester) external view returns (bool);
        function getNumEnabledAttesters() external view returns (uint256);
    }
}

pub struct EvmAttesterSetReader {
    rpc: EvmRpcClient,
    contract: Address,
}

impl EvmAttesterSetReader {
    pub fn new(config: &CctpConfig) -> Result<Self, AttesterSetError> {
        if config.sepolia_rpc_url.trim().is_empty() {
            return Err(AttesterSetError::NotReady);
        }
        let rpc = EvmRpcClient::new(&config.sepolia_rpc_url)
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        let contract: Address = config
            .contracts
            .sepolia_message_transmitter
            .trim()
            .parse()
            .map_err(|_| AttesterSetError::Transient("contract address".into()))?;
        Ok(Self { rpc, contract })
    }

    async fn eth_call_u256(&self, data: &str) -> Result<u64, AttesterSetError> {
        let result = self
            .rpc
            .eth_call(&format!("{:#x}", self.contract), data, "latest")
            .await
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        let trimmed = result.trim_start_matches("0x");
        let bytes =
            hex::decode(trimmed).map_err(|_| AttesterSetError::Transient("decode hex".into()))?;
        if bytes.len() > 32 {
            return Err(AttesterSetError::Transient("value too large".into()));
        }
        let mut padded = [0u8; 32];
        padded[32 - bytes.len()..].copy_from_slice(&bytes);
        let value = u64::from_be_bytes(padded[24..32].try_into().unwrap());
        Ok(value)
    }

    async fn eth_call_bool(&self, data: &str) -> Result<bool, AttesterSetError> {
        let result = self
            .rpc
            .eth_call(&format!("{:#x}", self.contract), data, "latest")
            .await
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        Ok(result.ends_with('1'))
    }
}

#[async_trait]
impl AttesterSetReader for EvmAttesterSetReader {
    fn destination(&self) -> AttesterDestination {
        AttesterDestination::Sepolia
    }

    async fn read_snapshot(
        &self,
        iris_candidates: &[[u8; 20]],
        iris_set_hash: [u8; 32],
    ) -> Result<AttesterSetSnapshot, AttesterSetError> {
        self.rpc
            .ensure_chain()
            .await
            .map_err(|e| AttesterSetError::Transient(e.to_string()))?;
        if self.rpc.chain_id != SEPOLIA_CHAIN_ID_NUM {
            return Err(AttesterSetError::Transient("wrong chain".into()));
        }

        let threshold_data = IMessageTransmitterAttestable::signatureThresholdCall {}.abi_encode();
        let threshold = self
            .eth_call_u256(&format!("0x{}", hex::encode(threshold_data)))
            .await?;
        if threshold == 0 {
            return Err(AttesterSetError::ThresholdZero);
        }
        let threshold_u32 = u32::try_from(threshold)
            .map_err(|_| AttesterSetError::Transient("threshold overflow".into()))?;

        let mut enabled = Vec::new();
        for candidate in iris_candidates {
            let addr = Address::from_slice(candidate);
            let data = IMessageTransmitterAttestable::isEnabledAttesterCall { attester: addr }
                .abi_encode();
            let ok = self
                .eth_call_bool(&format!("0x{}", hex::encode(data)))
                .await?;
            if ok {
                if !iris_candidates.contains(candidate) {
                    return Err(AttesterSetError::OnChainNotInIris);
                }
                enabled.push(*candidate);
            }
        }
        enabled.sort();
        enabled.dedup();
        if (enabled.len() as u32) < threshold_u32 {
            return Err(AttesterSetError::InsufficientEnabled);
        }

        Ok(AttesterSetSnapshot {
            destination: AttesterDestination::Sepolia,
            signature_threshold: threshold_u32,
            enabled_addresses: enabled,
            iris_set_hash,
            verified_at: std::time::Instant::now(),
            block_or_ledger: Some("latest".into()),
            source: "evm_message_transmitter_v2",
        })
    }
}

pub fn evm_reader_arc(config: &CctpConfig) -> Result<Arc<dyn AttesterSetReader>, AttesterSetError> {
    Ok(Arc::new(EvmAttesterSetReader::new(config)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_rpc() {
        let mut cfg = CctpConfig::default_testnet();
        cfg.sepolia_rpc_url.clear();
        assert!(matches!(
            EvmAttesterSetReader::new(&cfg),
            Err(AttesterSetError::NotReady)
        ));
    }
}
