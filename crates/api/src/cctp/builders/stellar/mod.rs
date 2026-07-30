//! Stellar Testnet Soroban unsigned CCTP transaction builders.
//!
//! Production builder requires Soroban RPC simulation; offline encoder is test-only.

pub mod encoder;

use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::cctp::builders::{
    BuilderError, BurnPrepareStep, PreparedBurnBundle, PreparedMintBundle, StellarCctpBurnBuilder,
    StellarCctpMintBuilder,
};
use crate::cctp::config::{CctpConfig, STELLAR_TESTNET_PASSPHRASE};
use crate::cctp::encoding::{
    cctp_subunits_to_stellar_subunits, evm_address_to_bytes32, stellar_outbound_cctp_amount,
};
use crate::cctp::store::CctpTransfer;
use crate::models::v2_cctp::{CctpDirection, PreparedWalletPayload};
use crate::simulation::{SimulationConfig, SorobanSimulator};
use crate::swap::tx::AccountSequenceSource;

use crate::cctp::stellar_payload::passphrase_for_config;
use encoder::{
    approve_args, deposit_for_burn_args, encode_invoke_at_sequence, mint_and_forward_args,
};

/// Soroban token allowance probe — production implementations query on-chain state.
#[async_trait]
pub trait StellarAllowanceChecker: Send + Sync {
    async fn has_sufficient_allowance(
        &self,
        owner: &str,
        token: &str,
        spender: &str,
        amount: i128,
    ) -> Result<bool, BuilderError>;
}

/// Test double for allowance gating.
pub struct FixedAllowanceChecker {
    pub sufficient: bool,
}

#[async_trait]
impl StellarAllowanceChecker for FixedAllowanceChecker {
    async fn has_sufficient_allowance(
        &self,
        _owner: &str,
        _token: &str,
        _spender: &str,
        _amount: i128,
    ) -> Result<bool, BuilderError> {
        Ok(self.sufficient)
    }
}

/// Offline XDR encoder — not production-ready; never enters runtime aggregate.
pub struct OfflineStellarXdrEncoder;

impl OfflineStellarXdrEncoder {
    pub fn encode_approval_at_sequence(
        source: &str,
        token: &str,
        spender: &str,
        amount: i128,
        ledger_sequence: i64,
    ) -> Result<String, BuilderError> {
        encode_invoke_at_sequence(
            source,
            token,
            "approve",
            approve_args(spender, amount)?,
            ledger_sequence,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_burn_at_sequence(
        source: &str,
        token_messenger: &str,
        caller: &str,
        amount: i128,
        destination_domain: u32,
        mint_recipient: [u8; 32],
        burn_token: &str,
        max_fee: i128,
        ledger_sequence: i64,
    ) -> Result<String, BuilderError> {
        encode_invoke_at_sequence(
            source,
            token_messenger,
            "deposit_for_burn",
            deposit_for_burn_args(
                caller,
                amount,
                destination_domain,
                mint_recipient,
                burn_token,
                max_fee,
            )?,
            ledger_sequence,
        )
    }
}

#[async_trait]
impl StellarCctpBurnBuilder for OfflineStellarXdrEncoder {
    fn is_ready(&self) -> bool {
        false
    }

    async fn prepare_burn(
        &self,
        _: &CctpTransfer,
        _: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError> {
        Err(BuilderError::NotReady)
    }
}

pub struct ProductionStellarCctpBuilder {
    pub sequences: std::sync::Arc<dyn AccountSequenceSource>,
    pub simulator: std::sync::Arc<SorobanSimulator>,
    pub allowance: std::sync::Arc<dyn StellarAllowanceChecker>,
    pub rpc_url: String,
}

impl ProductionStellarCctpBuilder {
    pub fn from_config(
        config: &CctpConfig,
        sequences: std::sync::Arc<dyn AccountSequenceSource>,
        allowance: std::sync::Arc<dyn StellarAllowanceChecker>,
    ) -> Result<Self, BuilderError> {
        let simulator = SorobanSimulator::new(SimulationConfig {
            rpc_url: config.stellar_rpc_url.clone(),
            ..Default::default()
        })
        .ok_or(BuilderError::NotReady)?;
        Ok(Self {
            sequences,
            simulator,
            allowance,
            rpc_url: config.stellar_rpc_url.clone(),
        })
    }

    fn burn_ready(&self) -> bool {
        !self.rpc_url.trim().is_empty()
    }

    fn ensure_testnet_config(config: &CctpConfig) -> Result<(), BuilderError> {
        let passphrase = if config.stellar_network_passphrase.is_empty() {
            STELLAR_TESTNET_PASSPHRASE
        } else {
            &config.stellar_network_passphrase
        };
        if passphrase != STELLAR_TESTNET_PASSPHRASE {
            return Err(BuilderError::Validation("wrong network passphrase".into()));
        }
        if config.stellar_domain != crate::cctp::config::STELLAR_TESTNET_DOMAIN {
            return Err(BuilderError::Validation("wrong stellar domain".into()));
        }
        Ok(())
    }

    fn ensure_not_expired(transfer: &CctpTransfer) -> Result<(), BuilderError> {
        if Utc::now() > transfer.quote_expires_at {
            return Err(BuilderError::QuoteExpired);
        }
        if let Some(fee_exp) = transfer.fee_expires_at {
            if Utc::now() > fee_exp {
                return Err(BuilderError::FeeExpired);
            }
        }
        Ok(())
    }

    async fn simulate_mandatory(&self, xdr: &str) -> Result<(), BuilderError> {
        let result = self.simulator.simulate(xdr).await;
        if !result.simulated {
            return Err(BuilderError::SimulationFailed(
                result
                    .failure_reason
                    .unwrap_or_else(|| "simulation not executed".into()),
            ));
        }
        if !result.success {
            return Err(BuilderError::SimulationFailed(
                result
                    .failure_reason
                    .unwrap_or_else(|| "simulation failed".into()),
            ));
        }
        Ok(())
    }

    async fn build_and_simulate(
        &self,
        source: &str,
        contract: &str,
        function: &str,
        args: Vec<stellar_xdr::curr::ScVal>,
        config: &CctpConfig,
    ) -> Result<String, BuilderError> {
        Self::ensure_testnet_config(config)?;
        let sequence = self
            .sequences
            .current_sequence(source)
            .await
            .map_err(|e| BuilderError::AccountLookup(e.to_string()))?;
        let xdr = encode_invoke_at_sequence(source, contract, function, args, sequence)?;
        self.simulate_mandatory(&xdr).await?;
        Ok(xdr)
    }

    fn stellar_payload(xdr: String, passphrase: &str) -> PreparedWalletPayload {
        PreparedWalletPayload::StellarXdr {
            network_passphrase: passphrase.to_string(),
            xdr_envelope: xdr,
        }
    }

    async fn needs_approval(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
        stellar_amount: i128,
    ) -> Result<bool, BuilderError> {
        if transfer.source_approval_verified_at.is_some() {
            return Ok(false);
        }
        let sufficient = self
            .allowance
            .has_sufficient_allowance(
                &transfer.sender,
                &config.contracts.stellar_usdc,
                &config.contracts.stellar_token_messenger,
                stellar_amount,
            )
            .await?;
        Ok(!sufficient)
    }
}

#[async_trait]
impl StellarCctpBurnBuilder for ProductionStellarCctpBuilder {
    fn is_ready(&self) -> bool {
        self.burn_ready()
    }

    async fn prepare_burn(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedBurnBundle, BuilderError> {
        if !self.burn_ready() {
            return Err(BuilderError::NotReady);
        }
        if transfer.direction != CctpDirection::StellarToEvm {
            return Err(BuilderError::Validation(
                "Stellar burn builder only supports stellar_to_evm".into(),
            ));
        }
        Self::ensure_not_expired(transfer)?;
        if transfer.sender.is_empty() {
            return Err(BuilderError::Validation(
                "sender required for Stellar burn".into(),
            ));
        }

        let (cctp_amount, _) = stellar_outbound_cctp_amount(&transfer.amount)
            .map_err(|e| BuilderError::Encoding(e.to_string()))?;
        let stellar_amount = cctp_subunits_to_stellar_subunits(cctp_amount)
            .map_err(|e| BuilderError::Encoding(e.to_string()))?
            as i128;
        let max_fee = transfer
            .max_fee
            .as_deref()
            .ok_or_else(|| BuilderError::Validation("max_fee missing".into()))?;
        let max_fee_stellar = cctp_subunits_to_stellar_subunits(
            crate::cctp::encoding::decimal_to_cctp_subunits(max_fee)
                .map_err(|e| BuilderError::Encoding(e.to_string()))?,
        )
        .map_err(|e| BuilderError::Encoding(e.to_string()))? as i128;

        let passphrase = if config.stellar_network_passphrase.is_empty() {
            STELLAR_TESTNET_PASSPHRASE.to_string()
        } else {
            config.stellar_network_passphrase.clone()
        };
        let expires_at = transfer.quote_expires_at.timestamp();

        if self
            .needs_approval(transfer, config, stellar_amount)
            .await?
        {
            let approve_xdr = self
                .build_and_simulate(
                    &transfer.sender,
                    &config.contracts.stellar_usdc,
                    "approve",
                    approve_args(&config.contracts.stellar_token_messenger, stellar_amount)?,
                    config,
                )
                .await?;
            return Ok(PreparedBurnBundle {
                step: BurnPrepareStep::Approval,
                approval_required: true,
                primary: Self::stellar_payload(approve_xdr, &passphrase),
                required_approvals: vec![],
                required_prior_payloads: vec![],
                expires_at,
            });
        }

        let mint_recipient = evm_address_to_bytes32(&transfer.recipient)
            .map_err(|e| BuilderError::Encoding(e.to_string()))?;
        let burn_xdr = self
            .build_and_simulate(
                &transfer.sender,
                &config.contracts.stellar_token_messenger,
                "deposit_for_burn",
                deposit_for_burn_args(
                    &transfer.sender,
                    stellar_amount,
                    config.sepolia_domain,
                    mint_recipient,
                    &config.contracts.stellar_usdc,
                    max_fee_stellar,
                )?,
                config,
            )
            .await?;

        Ok(PreparedBurnBundle {
            step: BurnPrepareStep::Burn,
            approval_required: false,
            primary: Self::stellar_payload(burn_xdr, &passphrase),
            required_approvals: vec![],
            required_prior_payloads: vec![],
            expires_at,
        })
    }
}

#[async_trait]
impl StellarCctpMintBuilder for ProductionStellarCctpBuilder {
    fn is_ready(&self) -> bool {
        self.burn_ready()
    }

    async fn prepare_mint(
        &self,
        transfer: &CctpTransfer,
        config: &CctpConfig,
    ) -> Result<PreparedMintBundle, BuilderError> {
        if !self.burn_ready() {
            return Err(BuilderError::NotReady);
        }
        if transfer.direction != CctpDirection::EvmToStellar {
            return Err(BuilderError::Validation(
                "Stellar mint builder only supports evm_to_stellar destination".into(),
            ));
        }
        let message = transfer
            .raw_message
            .as_ref()
            .ok_or_else(|| BuilderError::Validation("raw_message missing".into()))?;
        let attestation = transfer
            .attestation
            .as_ref()
            .ok_or_else(|| BuilderError::Validation("attestation missing".into()))?;

        if transfer.recipient.is_empty() {
            return Err(BuilderError::Validation("recipient required".into()));
        }

        let xdr = self
            .build_and_simulate(
                &transfer.recipient,
                &config.contracts.stellar_cctp_forwarder,
                "mint_and_forward",
                mint_and_forward_args(message, attestation)?,
                config,
            )
            .await?;

        let payload = Self::stellar_payload(xdr, &passphrase_for_config(config));
        let json = serde_json::to_string(&payload).unwrap_or_default();
        let payload_hash = hex::encode(Sha256::digest(json.as_bytes()));
        let expires_at = (Utc::now() + chrono::Duration::minutes(10)).timestamp();

        Ok(PreparedMintBundle {
            primary: payload,
            expires_at,
            payload_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::encoder::envelope_sequence;
    use super::*;
    use crate::cctp::config::CctpConfig;
    use crate::simulation::{SimulationConfig, SorobanSimulator};
    use crate::swap::tx::FixedAccountSequences;
    use chrono::Duration;
    use std::sync::Arc;
    use uuid::Uuid;

    fn test_simulator() -> Arc<SorobanSimulator> {
        SorobanSimulator::new(SimulationConfig {
            rpc_url: "http://127.0.0.1:1".into(),
            ..Default::default()
        })
        .expect("simulator")
    }
    fn sample_stellar_burn_transfer(approval_hash: Option<String>) -> CctpTransfer {
        let now = Utc::now();
        CctpTransfer {
            transfer_id: Uuid::new_v4(),
            support_reference_id: "sup".into(),
            corridor_id: "c".into(),
            provider: "circle-cctp".into(),
            direction: CctpDirection::StellarToEvm,
            source_chain_id: "stellar:testnet".into(),
            destination_chain_id: "eip155:11155111".into(),
            source_asset: "a".into(),
            source_asset_canonical: "a".into(),
            destination_asset: "b".into(),
            destination_asset_canonical: "b".into(),
            sender: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".into(),
            recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
            amount: "1.0000000".into(),
            destination_amount: "1.0000000".into(),
            finality: crate::models::v2_cctp::CctpFinality::Standard,
            runtime_fee_quote: Some("1".into()),
            max_fee: Some("1".into()),
            fee_expires_at: Some(now + Duration::minutes(10)),
            quote_expires_at: now + Duration::minutes(10),
            status: crate::models::v2_cctp::CctpTransferStatus::BurnPrepared,
            source_tx_hash: None,
            source_approval_tx_hash: approval_hash,
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

    #[test]
    fn offline_encoder_uses_distinct_sequences_for_approval_then_burn() {
        let source = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        let cfg = CctpConfig::default_testnet();
        let approve = OfflineStellarXdrEncoder::encode_approval_at_sequence(
            source,
            &cfg.contracts.stellar_usdc,
            &cfg.contracts.stellar_token_messenger,
            1_000_000,
            100,
        )
        .unwrap();
        let burn = OfflineStellarXdrEncoder::encode_burn_at_sequence(
            source,
            &cfg.contracts.stellar_token_messenger,
            source,
            1_000_000,
            cfg.sepolia_domain,
            [1u8; 32],
            &cfg.contracts.stellar_usdc,
            1,
            101,
        )
        .unwrap();
        assert_eq!(envelope_sequence(&approve).unwrap(), 100);
        assert_eq!(envelope_sequence(&burn).unwrap(), 101);
        assert_ne!(approve, burn);
    }

    #[test]
    fn offline_encoder_is_not_production_ready() {
        assert!(!OfflineStellarXdrEncoder.is_ready());
    }

    #[tokio::test]
    async fn approval_gate_returns_only_approval_payload_when_allowance_insufficient() {
        let builder = ProductionStellarCctpBuilder {
            sequences: Arc::new(FixedAccountSequences::new(100)),
            simulator: test_simulator(),
            allowance: Arc::new(FixedAllowanceChecker { sufficient: false }),
            rpc_url: "https://soroban-testnet.stellar.org".into(),
        };
        // Production builder requires simulation — wiremock test covers full path.
        // Here we verify allowance gate logic via needs_approval + encoder separation.
        let transfer = sample_stellar_burn_transfer(None);
        let needs = builder
            .needs_approval(&transfer, &CctpConfig::default_testnet(), 1)
            .await
            .unwrap();
        assert!(needs);
        let with_approval = sample_stellar_burn_transfer(Some("stellar-approval-hash".into()));
        let mut verified = with_approval;
        verified.source_approval_verified_at = Some(Utc::now());
        let needs_after = builder
            .needs_approval(&verified, &CctpConfig::default_testnet(), 1)
            .await
            .unwrap();
        assert!(!needs_after);
    }

    #[tokio::test]
    async fn production_not_ready_without_rpc_url() {
        let builder = ProductionStellarCctpBuilder {
            sequences: Arc::new(FixedAccountSequences::new(1)),
            simulator: test_simulator(),
            allowance: Arc::new(FixedAllowanceChecker { sufficient: true }),
            rpc_url: String::new(),
        };
        assert!(!StellarCctpBurnBuilder::is_ready(&builder));
        let err = builder
            .prepare_burn(
                &sample_stellar_burn_transfer(None),
                &CctpConfig::default_testnet(),
            )
            .await
            .unwrap_err();
        assert_eq!(err, BuilderError::NotReady);
    }
}
