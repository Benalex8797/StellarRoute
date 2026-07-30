//! Non-mutating Stellar CCTP contract readiness probes beyond RPC reachability.

use std::sync::Arc;

use stellar_xdr::curr::ScVal;

use crate::cctp::config::CctpConfig;
use crate::cctp::stellar_rpc::StellarRpcClient;
use crate::cctp::verifiers::VerifierError;

#[derive(Debug, Clone, Default)]
pub struct StellarContractProbeResult {
    pub rpc_ok: bool,
    pub message_transmitter_ok: bool,
    pub forwarder_ok: bool,
    pub token_messenger_ok: bool,
    pub usdc_ok: bool,
}

impl StellarContractProbeResult {
    pub fn all_ok(&self) -> bool {
        self.rpc_ok
            && self.message_transmitter_ok
            && self.forwarder_ok
            && self.token_messenger_ok
            && self.usdc_ok
    }
}

pub async fn probe_stellar_contracts(config: &CctpConfig) -> StellarContractProbeResult {
    let mut out = StellarContractProbeResult::default();
    let Ok(rpc) = StellarRpcClient::new(config) else {
        return out;
    };
    let rpc = Arc::new(rpc);
    out.rpc_ok = rpc.latest_ledger().await.is_ok();
    if !out.rpc_ok {
        return out;
    }

    let zero_nonce = [0u8; 32];
    out.message_transmitter_ok = rpc
        .simulate_is_nonce_used(&config.contracts.stellar_message_transmitter, zero_nonce)
        .await
        .is_ok();

    out.forwarder_ok = rpc
        .simulate_scval(
            &config.contracts.stellar_cctp_forwarder,
            "get_message_transmitter",
            vec![],
        )
        .await
        .is_ok();

    out.token_messenger_ok = rpc
        .simulate_scval(&config.contracts.stellar_token_messenger, "paused", vec![])
        .await
        .is_ok();

    out.usdc_ok = rpc
        .simulate_scval(&config.contracts.stellar_usdc, "decimals", vec![])
        .await
        .is_ok();

    out
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "diagnostic — live Stellar RPC contract probe fields"]
    async fn live_probe_fields() {
        let cfg = crate::cctp::config::CctpConfig::default_testnet();
        let out = probe_stellar_contracts(&cfg).await;
        eprintln!("{out:?}");
        assert!(out.all_ok());
    }
}

pub async fn simulate_allowance(
    rpc: &StellarRpcClient,
    token: &str,
    owner: &str,
    spender: &str,
) -> Result<i128, VerifierError> {
    use crate::cctp::builders::stellar::encoder::{account_address, contract_address};
    let owner_addr = account_address(owner).map_err(|e| VerifierError::Failed(e.to_string()))?;
    let spender_addr =
        contract_address(spender).map_err(|e| VerifierError::Failed(e.to_string()))?;
    let val = rpc
        .simulate_scval(
            token,
            "allowance",
            vec![ScVal::Address(owner_addr), ScVal::Address(spender_addr)],
        )
        .await?;
    crate::cctp::stellar_contract_events::scval_to_i128(&val)
}
