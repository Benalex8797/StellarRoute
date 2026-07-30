//! HTTP execution gate, redacted wire mapping, and service error translation.

use chrono::Utc;
use uuid::Uuid;

use crate::cctp::access::{access_tokens_match, validate_access_token_format};
use crate::cctp::builders::BurnPrepareStep;
use crate::cctp::config::CctpConfig;
use crate::cctp::readiness::CctpRuntime;
use crate::cctp::service::{CctpService, CctpServiceError};
use crate::cctp::store::CctpTransfer;
use crate::cctp::transitions::is_recoverable_failure;
use crate::cctp::verifiers::VerifierError;
use crate::error::ApiError;
use crate::metrics;
use crate::models::v2_cctp::SupportedCorridor;
use crate::models::v2_cctp::{
    CctpDirection, CctpFeeQuote, CctpPrepareBurnResponse, CctpPrepareMintResponse,
    CctpQuoteResponse, CctpReattestResponse, CctpStatusDetails, CctpSubmitBurnResponse,
    CctpSubmitMintResponse, CctpTransferStatus, CctpTransferStatusResponse, CctpValidationError,
    CCTP_PROVIDER_ID, CCTP_TESTNET_CORRIDOR_ID, SEPOLIA_CHAIN_ID, SEPOLIA_USDC_ASSET,
    SEPOLIA_USDC_CANONICAL, STELLAR_TESTNET_CHAIN_ID, STELLAR_TESTNET_USDC_ASSET,
    STELLAR_TESTNET_USDC_CANONICAL,
};

pub const REATTEST_MAX_ATTEMPTS: u32 = 5;
pub const REATTEST_COOLDOWN_SECS: i64 = 60;

/// Direction-specific executability — all mandatory prepare+verify components ready.
pub fn direction_executable(
    runtime: &CctpRuntime,
    config: &CctpConfig,
    direction: CctpDirection,
) -> bool {
    config.enabled && config.is_configured() && runtime.assess(direction).is_ready()
}

pub fn any_direction_executable(runtime: &CctpRuntime, config: &CctpConfig) -> bool {
    direction_executable(runtime, config, CctpDirection::StellarToEvm)
        || direction_executable(runtime, config, CctpDirection::EvmToStellar)
}

pub fn supported_corridors(runtime: &CctpRuntime, config: &CctpConfig) -> Vec<SupportedCorridor> {
    vec![
        corridor_descriptor(CctpDirection::StellarToEvm, runtime, config),
        corridor_descriptor(CctpDirection::EvmToStellar, runtime, config),
    ]
}

fn corridor_descriptor(
    direction: CctpDirection,
    runtime: &CctpRuntime,
    config: &CctpConfig,
) -> SupportedCorridor {
    let (source_chain_id, destination_chain_id, source_asset, destination_asset) = match direction {
        CctpDirection::StellarToEvm => (
            STELLAR_TESTNET_CHAIN_ID,
            SEPOLIA_CHAIN_ID,
            stellar_usdc_asset(),
            sepolia_usdc_asset(),
        ),
        CctpDirection::EvmToStellar => (
            SEPOLIA_CHAIN_ID,
            STELLAR_TESTNET_CHAIN_ID,
            sepolia_usdc_asset(),
            stellar_usdc_asset(),
        ),
    };

    SupportedCorridor {
        corridor_id: CCTP_TESTNET_CORRIDOR_ID.into(),
        provider: CCTP_PROVIDER_ID.into(),
        direction,
        source_chain_id: source_chain_id.into(),
        destination_chain_id: destination_chain_id.into(),
        source_asset,
        destination_asset,
        executable: direction_executable(runtime, config, direction),
    }
}

fn stellar_usdc_asset() -> crate::models::v2_cctp::CctpChainAsset {
    crate::models::v2_cctp::CctpChainAsset {
        chain_id: STELLAR_TESTNET_CHAIN_ID.into(),
        asset: STELLAR_TESTNET_USDC_ASSET.into(),
        canonical: STELLAR_TESTNET_USDC_CANONICAL.into(),
        symbol: Some("USDC".into()),
    }
}

fn sepolia_usdc_asset() -> crate::models::v2_cctp::CctpChainAsset {
    crate::models::v2_cctp::CctpChainAsset {
        chain_id: SEPOLIA_CHAIN_ID.into(),
        asset: SEPOLIA_USDC_ASSET.into(),
        canonical: SEPOLIA_USDC_CANONICAL.into(),
        symbol: Some("USDC".into()),
    }
}

/// Fail-closed gate before mutating saga state or returning executable payloads.
pub async fn ensure_public_gate(
    service: &CctpService,
    direction: CctpDirection,
) -> Result<(), ApiError> {
    let config = &service.config;
    if !config.enabled {
        metrics::record_cctp_endpoint_outcome("gate", "not_enabled");
        return Err(ApiError::CctpNotEnabled(
            "Circle CCTP bridge settlement is not enabled on this deployment".into(),
        ));
    }
    if !config.is_configured() {
        metrics::record_cctp_endpoint_outcome("gate", "not_configured");
        return Err(ApiError::CctpNotEnabled(
            "Circle CCTP bridge is not fully configured on this deployment".into(),
        ));
    }
    if service.provider_killed().await {
        metrics::record_cctp_endpoint_outcome("gate", "provider_killed");
        return Err(ApiError::ProviderKilled(
            "Circle CCTP provider is temporarily unavailable".into(),
        ));
    }
    if !direction_executable(&service.runtime, config, direction) {
        metrics::record_cctp_endpoint_outcome("gate", "direction_not_ready");
        return Err(ApiError::CctpNotEnabled(
            "Circle CCTP corridor dependencies are not ready for this direction".into(),
        ));
    }
    Ok(())
}

pub fn verify_transfer_access(
    transfer: &CctpTransfer,
    presented_token: Option<&str>,
) -> Result<(), ApiError> {
    let Some(hash) = transfer.access_token_hash.as_deref() else {
        metrics::record_cctp_endpoint_outcome("access", "missing_binding");
        return Err(ApiError::Unauthorized(
            "Transfer access token required".into(),
        ));
    };
    let Some(token) = presented_token else {
        metrics::record_cctp_endpoint_outcome("access", "missing_header");
        return Err(ApiError::Unauthorized(
            "Transfer access token required".into(),
        ));
    };
    if validate_access_token_format(token).is_err() || !access_tokens_match(hash, token) {
        metrics::record_cctp_endpoint_outcome("access", "invalid");
        return Err(ApiError::Unauthorized(
            "Invalid transfer access token".into(),
        ));
    }
    Ok(())
}

pub fn ensure_reattest_allowed(transfer: &CctpTransfer) -> Result<(), ApiError> {
    if !is_recoverable_failure(transfer.status) {
        return Err(ApiError::Validation(
            "Re-attestation is only allowed from attestation_failed state".into(),
        ));
    }
    if transfer.retry_count >= REATTEST_MAX_ATTEMPTS {
        return Err(ApiError::Validation(
            "Re-attestation attempt limit reached".into(),
        ));
    }
    let cooldown_deadline = transfer.updated_at + chrono::Duration::seconds(REATTEST_COOLDOWN_SECS);
    if Utc::now() < cooldown_deadline {
        return Err(ApiError::Validation(
            "Re-attestation cooldown active; retry later".into(),
        ));
    }
    Ok(())
}

pub fn map_service_error(err: CctpServiceError, transfer_id: Option<Uuid>) -> ApiError {
    match err {
        CctpServiceError::NotEnabled => ApiError::CctpNotEnabled(
            "Circle CCTP bridge settlement is not enabled on this deployment".into(),
        ),
        CctpServiceError::ProviderKilled => {
            ApiError::ProviderKilled("Circle CCTP provider is temporarily unavailable".into())
        }
        CctpServiceError::Validation(v) => map_validation(v),
        CctpServiceError::FeeQuoteUnavailable => {
            ApiError::FeeQuoteUnavailable("Runtime CCTP fee quote is unavailable".into())
        }
        CctpServiceError::VerifiersNotReady => ApiError::CctpNotEnabled(
            "Circle CCTP verifiers are not ready on this deployment".into(),
        ),
        CctpServiceError::NotFound => ApiError::TransferNotFound {
            transfer_id: transfer_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "unknown".into()),
        },
        CctpServiceError::InvalidState => {
            ApiError::Validation("Transfer is not in a valid state for this operation".into())
        }
        CctpServiceError::QuoteExpired => {
            ApiError::Validation("CCTP quote has expired; request a new quote".into())
        }
        CctpServiceError::FeeExpired => {
            ApiError::FeeQuoteUnavailable("CCTP fee quote has expired; request a new quote".into())
        }
        CctpServiceError::MintPayloadExpired => {
            ApiError::Validation("Mint payload has expired; call prepare-mint again".into())
        }
        CctpServiceError::AttestationTimeout => ApiError::AttestationExpired {
            transfer_id: transfer_id.map(|id| id.to_string()).unwrap_or_default(),
        },
        CctpServiceError::MissingAttestation | CctpServiceError::Attestation(_) => {
            ApiError::AttestationPending {
                transfer_id: transfer_id.map(|id| id.to_string()).unwrap_or_default(),
            }
        }
        CctpServiceError::MintRetryable => ApiError::MintRetryable {
            transfer_id: transfer_id.map(|id| id.to_string()).unwrap_or_default(),
        },
        CctpServiceError::ActivePrepareExists => {
            ApiError::Validation("An active prepare already exists for this source account".into())
        }
        CctpServiceError::AmountExceedsCap => {
            ApiError::InvalidAmount("amount exceeds configured cap".into())
        }
        CctpServiceError::FastNotSupported => ApiError::InvalidFinality,
        CctpServiceError::StellarRemainder => ApiError::Validation(
            "Stellar outbound amount must have zero 7th-decimal remainder".into(),
        ),
        CctpServiceError::Iris(_) => ApiError::DependencyUnavailable(
            "Circle attestation service is temporarily unavailable".into(),
        ),
        CctpServiceError::Verifier(VerifierError::Transient(_)) => ApiError::DependencyUnavailable(
            "On-chain verification dependency is temporarily unavailable".into(),
        ),
        CctpServiceError::Verifier(_)
        | CctpServiceError::Builder(_)
        | CctpServiceError::InvalidMessage
        | CctpServiceError::IrisTxHashMismatch
        | CctpServiceError::MintPayloadHashMismatch => {
            ApiError::Validation("On-chain verification failed for submitted transaction".into())
        }
        CctpServiceError::Store(_) => ApiError::Internal(std::sync::Arc::new(anyhow::anyhow!(
            "CCTP persistence error"
        ))),
    }
}

fn map_validation(err: CctpValidationError) -> ApiError {
    match err {
        CctpValidationError::UnsupportedCorridor => ApiError::UnsupportedCorridor,
        CctpValidationError::InvalidFinality => ApiError::InvalidFinality,
        CctpValidationError::InvalidRecipient => ApiError::InvalidRecipient,
        CctpValidationError::InvalidAmount => {
            ApiError::InvalidAmount("amount must be a positive decimal string".into())
        }
        CctpValidationError::InvalidSender => ApiError::Validation(
            "sender must be a valid G-address for Stellar or 0x address for EVM source".into(),
        ),
        CctpValidationError::InvalidMintSubmitter => ApiError::Validation(
            "mint_submitter must be a valid Stellar G-address for evm_to_stellar".into(),
        ),
        CctpValidationError::StellarRemainder => ApiError::Validation(
            "Stellar outbound amount must have zero 7th-decimal remainder".into(),
        ),
    }
}

pub fn to_quote_response(transfer: &CctpTransfer, access_token: &str) -> CctpQuoteResponse {
    CctpQuoteResponse {
        transfer_id: transfer.transfer_id.to_string(),
        corridor_id: transfer.corridor_id.clone(),
        provider: transfer.provider.clone(),
        direction: transfer.direction,
        source_amount: transfer.amount.clone(),
        destination_amount: transfer.destination_amount.clone(),
        fee_quote: fee_quote_from_transfer(transfer),
        expires_at: transfer.quote_expires_at.timestamp(),
        finality: transfer.finality,
        access_token: access_token.to_string(),
    }
}

pub fn to_status_response(transfer: &CctpTransfer) -> CctpTransferStatusResponse {
    let retryable = matches!(
        transfer.status,
        CctpTransferStatus::AttestationFailed
            | CctpTransferStatus::MintFailedRetryable
            | CctpTransferStatus::AwaitingAttestation
            | CctpTransferStatus::MintSubmitted
    );

    CctpTransferStatusResponse {
        transfer_id: transfer.transfer_id.to_string(),
        corridor_id: transfer.corridor_id.clone(),
        provider: transfer.provider.clone(),
        direction: transfer.direction,
        status: transfer.status,
        source_tx_hash: transfer.source_tx_hash.clone(),
        destination_tx_hash: transfer.destination_tx_hash.clone(),
        support_reference_id: Some(transfer.support_reference_id.clone()),
        retryable,
        error: redacted_status_error(transfer),
    }
}

fn redacted_status_error(transfer: &CctpTransfer) -> Option<CctpStatusDetails> {
    let code = transfer.last_provider_code.as_deref()?;
    let safe_codes = [
        "poll_timeout",
        "429",
        "mint_retryable",
        "mint_reconciliation_nonce",
        "attestation_pending",
    ];
    if !safe_codes.contains(&code) {
        return None;
    }
    let retryable = matches!(
        transfer.status,
        CctpTransferStatus::AttestationFailed
            | CctpTransferStatus::MintFailedRetryable
            | CctpTransferStatus::AwaitingAttestation
    );
    Some(CctpStatusDetails {
        code: code.to_string(),
        message: sanitized_provider_message(transfer.last_provider_error.as_deref()),
        retryable: Some(retryable),
    })
}

fn sanitized_provider_message(raw: Option<&str>) -> String {
    match raw {
        Some(msg) if !msg.contains("http") && !msg.contains("0x") && msg.len() <= 200 => {
            msg.to_string()
        }
        _ => "Provider operation pending or failed".into(),
    }
}

fn fee_quote_from_transfer(transfer: &CctpTransfer) -> CctpFeeQuote {
    let fee_asset = match transfer.direction {
        CctpDirection::StellarToEvm => Some(stellar_usdc_asset()),
        CctpDirection::EvmToStellar => Some(sepolia_usdc_asset()),
    };
    CctpFeeQuote {
        source_fee: transfer.runtime_fee_quote.clone(),
        destination_fee: None,
        bridge_fee: transfer.max_fee.clone(),
        fee_asset,
    }
}

pub fn to_prepare_burn_response(
    transfer: &CctpTransfer,
    bundle: &crate::cctp::builders::PreparedBurnBundle,
) -> CctpPrepareBurnResponse {
    CctpPrepareBurnResponse {
        transfer_id: transfer.transfer_id.to_string(),
        status: transfer.status,
        payload: bundle.primary.clone(),
        expires_at: bundle.expires_at,
        approval_required: bundle.step == BurnPrepareStep::Approval,
    }
}

pub fn to_prepare_mint_response(
    transfer: &CctpTransfer,
    bundle: &crate::cctp::builders::PreparedMintBundle,
) -> CctpPrepareMintResponse {
    CctpPrepareMintResponse {
        transfer_id: transfer.transfer_id.to_string(),
        status: transfer.status,
        payload: bundle.primary.clone(),
        expires_at: bundle.expires_at,
    }
}

pub fn to_submit_burn_response(transfer: &CctpTransfer) -> CctpSubmitBurnResponse {
    CctpSubmitBurnResponse {
        transfer_id: transfer.transfer_id.to_string(),
        status: transfer.status,
        source_tx_hash: transfer
            .source_tx_hash
            .clone()
            .or_else(|| transfer.source_approval_tx_hash.clone())
            .unwrap_or_default(),
    }
}

pub fn to_submit_mint_response(transfer: &CctpTransfer) -> CctpSubmitMintResponse {
    CctpSubmitMintResponse {
        transfer_id: transfer.transfer_id.to_string(),
        status: transfer.status,
        destination_tx_hash: transfer.destination_tx_hash.clone().unwrap_or_default(),
    }
}

pub fn to_reattest_response(transfer: &CctpTransfer) -> CctpReattestResponse {
    CctpReattestResponse {
        transfer_id: transfer.transfer_id.to_string(),
        status: transfer.status,
        retryable: is_recoverable_failure(transfer.status)
            || transfer.status == CctpTransferStatus::AwaitingAttestation,
    }
}

/// Legacy helper for `/api/v2` info when only runtime is available (tests).
pub fn bridge_settlement_executable(runtime: &CctpRuntime, config: &CctpConfig) -> bool {
    any_direction_executable(runtime, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::access::generate_access_token;
    use chrono::Utc;

    fn sample_transfer(status: CctpTransferStatus) -> CctpTransfer {
        let (token, hash) = generate_access_token();
        let _ = token;
        CctpTransfer {
            transfer_id: Uuid::new_v4(),
            support_reference_id: "cctp-test".into(),
            corridor_id: CCTP_TESTNET_CORRIDOR_ID.into(),
            provider: CCTP_PROVIDER_ID.into(),
            direction: CctpDirection::StellarToEvm,
            source_chain_id: STELLAR_TESTNET_CHAIN_ID.into(),
            destination_chain_id: SEPOLIA_CHAIN_ID.into(),
            source_asset: STELLAR_TESTNET_USDC_ASSET.into(),
            source_asset_canonical: STELLAR_TESTNET_USDC_CANONICAL.into(),
            destination_asset: SEPOLIA_USDC_ASSET.into(),
            destination_asset_canonical: SEPOLIA_USDC_CANONICAL.into(),
            sender: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".into(),
            recipient: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0".into(),
            mint_submitter: None,
            amount: "1.0".into(),
            destination_amount: "1.0".into(),
            finality: crate::models::v2_cctp::CctpFinality::Standard,
            runtime_fee_quote: Some("1".into()),
            max_fee: Some("1".into()),
            fee_expires_at: Some(Utc::now()),
            quote_expires_at: Utc::now(),
            status,
            source_tx_hash: None,
            source_approval_tx_hash: None,
            source_approval_verified_at: None,
            destination_tx_hash: None,
            iris_message_hash: None,
            message_nonce: None,
            raw_message: Some(vec![1, 2, 3]),
            attestation: Some(vec![4, 5, 6]),
            retry_count: 0,
            last_provider_error: Some("secret http://evil".into()),
            last_provider_code: Some("poll_timeout".into()),
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            terminal_at: None,
            mint_payload_hash: None,
            mint_payload_expires_at: None,
            approval_payload_hash: None,
            approval_expiration_ledger: None,
            burn_payload_hash: None,
            burn_prepare_step: None,
            access_token_hash: Some(hash),
        }
    }

    #[test]
    fn status_response_never_leaks_raw_message_or_urls() {
        let json = serde_json::to_string(&to_status_response(&sample_transfer(
            CctpTransferStatus::AwaitingAttestation,
        )))
        .unwrap();
        assert!(!json.contains("raw_message"));
        assert!(!json.contains("\"attestation\""));
        assert!(!json.contains("http://"));
        assert!(!json.contains("access_token_hash"));
    }
}
