//! CCTP core service — quote, burn recording, attestation polling (no wallet tx builders).

use std::sync::Arc;

use chrono::{Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::cctp::attestation::{AttestationVerifier, AttestationVerifyError};
use crate::cctp::bounds::{check_byte_len, MAX_ATTESTATION_BYTES, MAX_RAW_MESSAGE_BYTES};
use crate::cctp::config::{CctpConfig, SEPOLIA_DOMAIN, STELLAR_TESTNET_DOMAIN};
use crate::cctp::encoding::{
    cctp_subunits_to_stellar_subunits, decimal_to_cctp_subunits, stellar_outbound_cctp_amount,
};
use crate::cctp::expectations::{build_corridor_expectations, build_expected_burn_facts};
use crate::cctp::iris::{IrisClient, IrisMessage, IrisMessageStatus, IrisPollOutcome};
use crate::cctp::message::{decode_hex_message, validate_message_for_corridor};
use crate::cctp::store::{CctpStoreError, CctpTransfer, CctpTransferStore, TransferPatch};
use crate::cctp::transitions::can_cancel;
use crate::cctp::verifiers::{facts_match, EvmBurnVerifier, StellarBurnVerifier, VerifierError};
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
    #[error("attestation: {0}")]
    Attestation(AttestationVerifyError),
    #[error("verifiers not ready")]
    VerifiersNotReady,
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
    #[error("attestation poll timeout")]
    AttestationTimeout,
    #[error("iris source tx hash mismatch")]
    IrisTxHashMismatch,
    #[error("missing attestation")]
    MissingAttestation,
}

pub struct CctpService {
    pub config: CctpConfig,
    pub store: Arc<dyn CctpTransferStore>,
    pub iris: Arc<dyn IrisClient>,
    pub kill_switch: Arc<KillSwitchManager>,
    pub stellar_verifier: Arc<dyn StellarBurnVerifier>,
    pub evm_verifier: Arc<dyn EvmBurnVerifier>,
    pub attestation_verifier: Arc<dyn AttestationVerifier>,
}

impl CctpService {
    pub fn burn_verifier_ready(&self, direction: CctpDirection) -> bool {
        match direction {
            CctpDirection::StellarToEvm => self.stellar_verifier.is_ready(),
            CctpDirection::EvmToStellar => self.evm_verifier.is_ready(),
        }
    }

    pub fn attestation_verifier_ready(&self) -> bool {
        self.attestation_verifier.is_ready()
    }

    pub fn core_verifiers_ready(&self, direction: CctpDirection) -> bool {
        self.burn_verifier_ready(direction) && self.attestation_verifier_ready()
    }

    fn ensure_burn_verifier_ready(&self, direction: CctpDirection) -> Result<(), CctpServiceError> {
        if !self.burn_verifier_ready(direction) {
            return Err(CctpServiceError::VerifiersNotReady);
        }
        Ok(())
    }

    fn ensure_attestation_verifier_ready(&self) -> Result<(), CctpServiceError> {
        if !self.attestation_verifier_ready() {
            return Err(CctpServiceError::VerifiersNotReady);
        }
        Ok(())
    }

    pub async fn provider_killed(&self) -> bool {
        let policy = self.kill_switch.get_provider_policy().await;
        !policy.is_provider_allowed(Some(self.config.provider_id()))
    }

    /// `created` → `burn_prepared` when config/verifiers allow (no wallet payload yet).
    pub async fn prepare_burn(&self, transfer_id: Uuid) -> Result<CctpTransfer, CctpServiceError> {
        if !self.config.enabled || !self.config.is_configured() {
            return Err(CctpServiceError::NotEnabled);
        }
        if self.provider_killed().await {
            metrics::record_cctp_provider_killed_new_transfer();
            return Err(CctpServiceError::ProviderKilled);
        }

        let transfer = self
            .store
            .get(transfer_id)
            .await
            .map_err(CctpServiceError::Store)?
            .ok_or(CctpServiceError::NotFound)?;

        if transfer.status != CctpTransferStatus::Created {
            return Err(CctpServiceError::InvalidState);
        }

        self.ensure_burn_verifier_ready(transfer.direction)?;

        let updated = self
            .store
            .transition(
                transfer_id,
                transfer.version,
                CctpTransferStatus::BurnPrepared,
                TransferPatch::default(),
            )
            .await
            .map_err(CctpServiceError::Store)?;

        metrics::record_cctp_transition("burn_prepared");
        Ok(updated)
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

        self.ensure_burn_verifier_ready(transfer.direction)?;

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

        let expected =
            build_expected_burn_facts(&transfer, &self.config, tx_hash).map_err(|_| {
                CctpServiceError::Verifier(VerifierError::Failed("expectations".into()))
            })?;

        if facts_match(&expected, &facts).is_err() {
            metrics::record_cctp_verifier_mismatch();
            return Err(CctpServiceError::Verifier(VerifierError::Failed(
                "burn facts mismatch".into(),
            )));
        }

        let awaiting = self
            .store
            .record_verified_burn(transfer_id, transfer.version, tx_hash)
            .await
            .map_err(CctpServiceError::Store)?;

        metrics::record_cctp_transition("awaiting_attestation");
        Ok(awaiting)
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

        if self.attestation_timed_out(&transfer) {
            return self.fail_attestation_timeout(transfer).await;
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
                self.validate_and_mark_attestation_ready(&transfer, &msg)
                    .await
            }
        }
    }

    fn attestation_timed_out(&self, transfer: &CctpTransfer) -> bool {
        let deadline =
            transfer.updated_at + Duration::seconds(self.config.poll_timeout_secs as i64);
        Utc::now() > deadline
    }

    async fn fail_attestation_timeout(
        &self,
        transfer: CctpTransfer,
    ) -> Result<CctpTransfer, CctpServiceError> {
        let updated = self
            .store
            .transition(
                transfer.transfer_id,
                transfer.version,
                CctpTransferStatus::AttestationFailed,
                TransferPatch {
                    last_provider_error: Some("attestation poll timeout".into()),
                    last_provider_code: Some("poll_timeout".into()),
                    ..Default::default()
                },
            )
            .await
            .map_err(CctpServiceError::Store)?;
        metrics::record_cctp_transition("attestation_failed");
        Ok(updated)
    }

    async fn validate_and_mark_attestation_ready(
        &self,
        transfer: &CctpTransfer,
        iris_msg: &IrisMessage,
    ) -> Result<CctpTransfer, CctpServiceError> {
        self.ensure_attestation_verifier_ready()?;

        if iris_msg.status != IrisMessageStatus::Complete {
            return Err(CctpServiceError::InvalidMessage);
        }

        let persisted = transfer
            .source_tx_hash
            .as_ref()
            .ok_or(CctpServiceError::InvalidState)?;
        let iris_hash = iris_msg
            .source_tx_hash
            .as_ref()
            .ok_or(CctpServiceError::IrisTxHashMismatch)?;
        if !tx_hashes_equal(persisted, iris_hash) {
            metrics::record_cctp_invalid_message();
            return Err(CctpServiceError::IrisTxHashMismatch);
        }

        let attestation_hex = iris_msg
            .attestation_hex
            .as_ref()
            .ok_or(CctpServiceError::MissingAttestation)?;
        if attestation_hex.is_empty() || attestation_hex.eq_ignore_ascii_case("PENDING") {
            return Err(CctpServiceError::MissingAttestation);
        }

        let expectations = build_corridor_expectations(transfer, &self.config).map_err(|_| {
            metrics::record_cctp_invalid_message();
            CctpServiceError::InvalidMessage
        })?;

        if validate_message_for_corridor(&iris_msg.message_hex, &expectations).is_err() {
            metrics::record_cctp_invalid_message();
            return Err(CctpServiceError::InvalidMessage);
        }

        let raw = decode_hex_message(&iris_msg.message_hex)
            .map_err(|_| CctpServiceError::InvalidMessage)?;
        if check_byte_len("raw_message", &raw, MAX_RAW_MESSAGE_BYTES).is_err() {
            return Err(CctpServiceError::InvalidMessage);
        }

        let attestation =
            decode_hex_message(attestation_hex).map_err(|_| CctpServiceError::InvalidMessage)?;
        if check_byte_len("attestation", &attestation, MAX_ATTESTATION_BYTES).is_err() {
            return Err(CctpServiceError::InvalidMessage);
        }

        self.attestation_verifier
            .verify_attestation(&raw, &attestation)
            .await
            .map_err(CctpServiceError::Attestation)?;

        if iris_msg.event_nonce.is_empty() {
            return Err(CctpServiceError::InvalidMessage);
        }

        let updated = self
            .store
            .transition(
                transfer.transfer_id,
                transfer.version,
                CctpTransferStatus::AttestationReady,
                TransferPatch {
                    raw_message: Some(raw),
                    attestation: Some(attestation),
                    message_nonce: Some(iris_msg.event_nonce.clone()),
                    ..Default::default()
                },
            )
            .await
            .map_err(CctpServiceError::Store)?;

        metrics::record_cctp_transition("attestation_ready");
        Ok(updated)
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
                    clear_terminal_at: true,
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
        self.poll_one_transfer(transfer_id).await?;
        Ok(())
    }
}

fn tx_hashes_equal(a: &str, b: &str) -> bool {
    normalize_tx_hash(a) == normalize_tx_hash(b)
}

fn normalize_tx_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    hex.to_ascii_lowercase()
}

fn format_stellar_amount(subunits: u128) -> String {
    let whole = subunits / 10_000_000;
    let frac = subunits % 10_000_000;
    format!("{}.{:07}", whole, frac)
}
