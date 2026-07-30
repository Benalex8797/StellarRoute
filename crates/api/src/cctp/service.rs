//! CCTP core service — quote, burn recording, attestation polling (no wallet tx builders).

use std::sync::Arc;

use chrono::{Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::cctp::config::{
    CctpConfig, FINALITY_FAST, FINALITY_STANDARD, SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN,
};
use crate::cctp::encoding::{
    cctp_subunits_to_stellar_subunits, decimal_to_cctp_subunits, stellar_contract_to_bytes32,
    stellar_outbound_cctp_amount,
};
use crate::cctp::iris::{IrisClient, IrisPollOutcome};
use crate::cctp::message::{
    decode_hex_message, validate_message_for_corridor, CorridorMessageExpectations,
};
use crate::cctp::store::{CctpStoreError, CctpTransfer, CctpTransferStore, TransferPatch};
use crate::cctp::transitions::can_cancel;
use crate::cctp::verifiers::{
    EvmBurnVerifier, StellarBurnVerifier, VerifiedBurnFacts, VerifierError,
};
use crate::kill_switch::KillSwitchManager;
use crate::metrics;
use crate::models::v2_cctp::{
    CctpDirection, CctpFinality, CctpQuoteRequest, CctpTransferStatus, CctpValidationError,
};

#[derive(Debug, Error)]
pub enum CctpServiceError {
    #[error("not enabled")]
    NotEnabled,
    #[error("provider killed")]
    ProviderKilled,
    #[error("validation: {0:?}")]
    Validation(CctpValidationError),
    #[error("fee quote unavailable")]
    FeeQuoteUnavailable,
    #[error("store: {0}")]
    Store(CctpStoreError),
    #[error("iris: {0}")]
    Iris(String),
    #[error("verifier: {0}")]
    Verifier(VerifierError),
    #[error("message validation failed")]
    InvalidMessage,
    #[error("amount exceeds cap")]
    AmountExceedsCap,
    #[error("fast finality not supported")]
    FastNotSupported,
    #[error("not found")]
    NotFound,
    #[error("invalid state")]
    InvalidState,
}

pub struct CctpService {
    pub config: CctpConfig,
    pub store: Arc<dyn CctpTransferStore>,
    pub iris: Arc<dyn IrisClient>,
    pub kill_switch: Arc<KillSwitchManager>,
    pub stellar_verifier: Arc<dyn StellarBurnVerifier>,
    pub evm_verifier: Arc<dyn EvmBurnVerifier>,
}

impl CctpService {
    pub async fn provider_killed(&self) -> bool {
        let policy = self.kill_switch.get_provider_policy().await;
        !policy.is_provider_allowed(Some(self.config.provider_id()))
    }

    /// Internal quote-core — not wired to HTTP handlers yet.
    pub async fn quote_core(
        &self,
        request: &CctpQuoteRequest,
    ) -> Result<CctpTransfer, CctpServiceError> {
        if !self.config.enabled {
            return Err(CctpServiceError::NotEnabled);
        }
        if self.provider_killed().await {
            metrics::record_cctp_provider_killed_new_transfer();
            return Err(CctpServiceError::ProviderKilled);
        }
        request.validate().map_err(CctpServiceError::Validation)?;

        if request.finality == CctpFinality::Fast {
            return Err(CctpServiceError::FastNotSupported);
        }

        let cctp_amount = match request.direction {
            CctpDirection::StellarToEvm => {
                let (amt, _) = stellar_outbound_cctp_amount(&request.amount).map_err(|_| {
                    CctpServiceError::Validation(CctpValidationError::InvalidAmount)
                })?;
                amt
            }
            CctpDirection::EvmToStellar => decimal_to_cctp_subunits(&request.amount)
                .map_err(|_| CctpServiceError::Validation(CctpValidationError::InvalidAmount))?,
        };

        let cap = decimal_to_cctp_subunits(&self.config.amount_cap).unwrap_or(u128::MAX);
        if cctp_amount > cap {
            return Err(CctpServiceError::AmountExceedsCap);
        }

        let (source_domain, dest_domain) = match request.direction {
            CctpDirection::StellarToEvm => (STELLAR_TESTNET_DOMAIN, SEPOLIA_DOMAIN),
            CctpDirection::EvmToStellar => (SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN),
        };

        let fees = self
            .iris
            .fetch_burn_fees(source_domain, dest_domain)
            .await
            .map_err(|e| CctpServiceError::Iris(e.to_string()))?;

        let max_fee = fees.standard_fee.clone();
        let now = Utc::now();
        let transfer_id = Uuid::new_v4();
        let support_id = format!("cctp-{}", transfer_id);

        let destination_amount = match request.direction {
            CctpDirection::StellarToEvm => request.amount.clone(),
            CctpDirection::EvmToStellar => {
                let stellar_sub = cctp_subunits_to_stellar_subunits(cctp_amount).map_err(|_| {
                    CctpServiceError::Validation(CctpValidationError::InvalidAmount)
                })?;
                format_stellar_amount(stellar_sub)
            }
        };

        let transfer = CctpTransfer {
            transfer_id,
            support_reference_id: support_id,
            corridor_id: request.corridor_id.clone(),
            provider: request.provider.clone(),
            direction: request.direction,
            source_chain_id: request.source_chain_id.clone(),
            destination_chain_id: request.destination_chain_id.clone(),
            source_asset: request.source_asset.asset.clone(),
            source_asset_canonical: request.source_asset.canonical.clone(),
            destination_asset: request.destination_asset.asset.clone(),
            destination_asset_canonical: request.destination_asset.canonical.clone(),
            sender: request.sender.clone().unwrap_or_default(),
            recipient: request.recipient.clone(),
            amount: request.amount.clone(),
            destination_amount,
            finality: request.finality,
            runtime_fee_quote: fees.standard_fee,
            max_fee,
            fee_expires_at: Some(now + Duration::minutes(10)),
            quote_expires_at: now + Duration::seconds(self.config.quote_ttl_secs as i64),
            status: CctpTransferStatus::Created,
            source_tx_hash: None,
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
        };

        self.store
            .insert(&transfer)
            .await
            .map_err(CctpServiceError::Store)?;
        metrics::record_cctp_transition("created");
        Ok(transfer)
    }

    pub async fn record_burn_submission(
        &self,
        transfer_id: Uuid,
        tx_hash: &str,
    ) -> Result<CctpTransfer, CctpServiceError> {
        let transfer = self
            .store
            .get(transfer_id)
            .await
            .map_err(CctpServiceError::Store)?
            .ok_or(CctpServiceError::NotFound)?;

        if transfer.status != CctpTransferStatus::BurnPrepared {
            return Err(CctpServiceError::InvalidState);
        }

        let facts = match transfer.direction {
            CctpDirection::StellarToEvm => self
                .stellar_verifier
                .verify_burn(tx_hash)
                .await
                .map_err(CctpServiceError::Verifier)?,
            CctpDirection::EvmToStellar => self
                .evm_verifier
                .verify_burn(tx_hash)
                .await
                .map_err(CctpServiceError::Verifier)?,
        };

        self.assert_burn_facts_match(&transfer, &facts)?;

        let updated = self
            .store
            .transition(
                transfer_id,
                transfer.version,
                CctpTransferStatus::BurnSubmitted,
                TransferPatch {
                    source_tx_hash: Some(tx_hash.to_string()),
                    ..Default::default()
                },
            )
            .await
            .map_err(CctpServiceError::Store)?;

        let awaiting = self
            .store
            .transition(
                transfer_id,
                updated.version,
                CctpTransferStatus::AwaitingAttestation,
                TransferPatch::default(),
            )
            .await
            .map_err(CctpServiceError::Store)?;

        metrics::record_cctp_transition("burn_submitted");
        metrics::record_cctp_transition("awaiting_attestation");
        Ok(awaiting)
    }

    fn assert_burn_facts_match(
        &self,
        transfer: &CctpTransfer,
        facts: &VerifiedBurnFacts,
    ) -> Result<(), CctpServiceError> {
        if facts.source_chain_id != transfer.source_chain_id {
            metrics::record_cctp_verifier_mismatch();
            return Err(CctpServiceError::Verifier(VerifierError::Failed(
                "chain mismatch".into(),
            )));
        }
        let expected_cctp = match transfer.direction {
            CctpDirection::StellarToEvm => stellar_outbound_cctp_amount(&transfer.amount)
                .map(|(a, _)| a)
                .unwrap_or(0),
            CctpDirection::EvmToStellar => decimal_to_cctp_subunits(&transfer.amount).unwrap_or(0),
        };
        if facts.amount_cctp_subunits != expected_cctp {
            metrics::record_cctp_verifier_mismatch();
            return Err(CctpServiceError::Verifier(VerifierError::Failed(
                "amount mismatch".into(),
            )));
        }
        Ok(())
    }

    pub async fn poll_one_transfer(
        &self,
        transfer_id: Uuid,
    ) -> Result<CctpTransfer, CctpServiceError> {
        let transfer = self
            .store
            .get(transfer_id)
            .await
            .map_err(CctpServiceError::Store)?
            .ok_or(CctpServiceError::NotFound)?;

        if transfer.status != CctpTransferStatus::AwaitingAttestation {
            return Ok(transfer);
        }

        let source_domain = match transfer.direction {
            CctpDirection::StellarToEvm => STELLAR_TESTNET_DOMAIN,
            CctpDirection::EvmToStellar => SEPOLIA_DOMAIN,
        };

        let tx_hash = transfer
            .source_tx_hash
            .as_ref()
            .ok_or(CctpServiceError::InvalidState)?;

        let start = std::time::Instant::now();
        let outcome = self
            .iris
            .poll_messages_by_tx(source_domain, tx_hash)
            .await
            .map_err(|e| CctpServiceError::Iris(e.to_string()))?;
        metrics::record_cctp_iris_latency(start.elapsed(), "poll");

        match outcome {
            IrisPollOutcome::Pending | IrisPollOutcome::NotFound => Ok(transfer),
            IrisPollOutcome::RateLimited { retry_after_secs } => {
                metrics::record_cctp_rate_limited();
                let _ = self
                    .store
                    .transition(
                        transfer_id,
                        transfer.version,
                        CctpTransferStatus::AwaitingAttestation,
                        TransferPatch {
                            last_provider_error: Some(format!(
                                "rate limited; retry after {retry_after_secs}s"
                            )),
                            last_provider_code: Some("429".into()),
                            ..Default::default()
                        },
                    )
                    .await;
                Ok(transfer)
            }
            IrisPollOutcome::Complete(msg) => {
                self.validate_and_mark_attestation_ready(&transfer, &msg.message_hex, &msg)
                    .await
            }
        }
    }

    async fn validate_and_mark_attestation_ready(
        &self,
        transfer: &CctpTransfer,
        message_hex: &str,
        iris_msg: &crate::cctp::iris::IrisMessage,
    ) -> Result<CctpTransfer, CctpServiceError> {
        let expectations = self.build_message_expectations(transfer)?;
        if validate_message_for_corridor(message_hex, &expectations).is_err() {
            metrics::record_cctp_invalid_message();
            return Err(CctpServiceError::InvalidMessage);
        }

        let raw = decode_hex_message(message_hex).map_err(|_| CctpServiceError::InvalidMessage)?;
        let attestation = iris_msg
            .attestation_hex
            .as_ref()
            .and_then(|a| decode_hex_message(a).ok());

        let updated = self
            .store
            .transition(
                transfer.transfer_id,
                transfer.version,
                CctpTransferStatus::AttestationReady,
                TransferPatch {
                    raw_message: Some(raw),
                    attestation,
                    message_nonce: Some(iris_msg.event_nonce.clone()),
                    ..Default::default()
                },
            )
            .await
            .map_err(CctpServiceError::Store)?;

        metrics::record_cctp_transition("attestation_ready");
        Ok(updated)
    }

    fn build_message_expectations(
        &self,
        transfer: &CctpTransfer,
    ) -> Result<CorridorMessageExpectations, CctpServiceError> {
        let (source_domain, dest_domain) = match transfer.direction {
            CctpDirection::StellarToEvm => (STELLAR_TESTNET_DOMAIN, SEPOLIA_DOMAIN),
            CctpDirection::EvmToStellar => (SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN),
        };

        let amount_cctp = match transfer.direction {
            CctpDirection::StellarToEvm => stellar_outbound_cctp_amount(&transfer.amount)
                .map(|(a, _)| a)
                .unwrap_or(0),
            CctpDirection::EvmToStellar => decimal_to_cctp_subunits(&transfer.amount).unwrap_or(0),
        };

        let burn_token = stellar_contract_to_bytes32(&self.config.contracts.stellar_usdc)
            .or_else(|_| stellar_contract_to_bytes32(&self.config.contracts.stellar_usdc))
            .unwrap_or([0u8; 32]);

        let min_finality = match transfer.finality {
            CctpFinality::Standard => FINALITY_STANDARD,
            CctpFinality::Fast => FINALITY_FAST,
        };

        Ok(CorridorMessageExpectations {
            source_domain,
            destination_domain: dest_domain,
            burn_token,
            mint_recipient: [0u8; 32],
            destination_caller: [0u8; 32],
            amount_cctp_subunits: amount_cctp,
            min_finality,
            hook_data: None,
        })
    }

    pub async fn reattest(&self, transfer_id: Uuid) -> Result<CctpTransfer, CctpServiceError> {
        let transfer = self
            .store
            .get(transfer_id)
            .await
            .map_err(CctpServiceError::Store)?
            .ok_or(CctpServiceError::NotFound)?;

        if transfer.status != CctpTransferStatus::AttestationFailed {
            return Err(CctpServiceError::InvalidState);
        }

        let nonce = transfer
            .message_nonce
            .as_ref()
            .ok_or(CctpServiceError::InvalidState)?;

        self.iris
            .reattest(nonce)
            .await
            .map_err(|e| CctpServiceError::Iris(e.to_string()))?;

        let updated = self
            .store
            .transition(
                transfer_id,
                transfer.version,
                CctpTransferStatus::AwaitingAttestation,
                TransferPatch {
                    increment_retry: true,
                    ..Default::default()
                },
            )
            .await
            .map_err(CctpServiceError::Store)?;

        metrics::record_cctp_transition("reattest_awaiting");
        Ok(updated)
    }

    pub async fn cancel(&self, transfer_id: Uuid) -> Result<CctpTransfer, CctpServiceError> {
        let transfer = self
            .store
            .get(transfer_id)
            .await
            .map_err(CctpServiceError::Store)?
            .ok_or(CctpServiceError::NotFound)?;

        if !can_cancel(transfer.status) {
            return Err(CctpServiceError::InvalidState);
        }

        let updated = self
            .store
            .transition(
                transfer_id,
                transfer.version,
                CctpTransferStatus::Cancelled,
                TransferPatch::default(),
            )
            .await
            .map_err(CctpServiceError::Store)?;
        metrics::record_cctp_transition("cancelled");
        Ok(updated)
    }

    /// Deterministic tick for worker wiring — polls one transfer if eligible.
    pub async fn tick_transfer(&self, transfer_id: Uuid) -> Result<(), CctpServiceError> {
        if self.provider_killed().await {
            // In-flight attestation polling allowed unless hard-killed at quote time.
        }
        self.poll_one_transfer(transfer_id).await?;
        Ok(())
    }
}

fn format_stellar_amount(subunits: u128) -> String {
    let whole = subunits / 10_000_000;
    let frac = subunits % 10_000_000;
    format!("{}.{:07}", whole, frac)
}
