//! Circle CCTP v2 bridge routes — contract freeze (fail-closed scaffolding).
//!
//! Every handler validates wire input where applicable, then returns a typed
//! `503 cctp_not_enabled` envelope. No fake quotes, prepares, or status data.

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::error::{ApiError, Result};
use crate::middleware::RequestId;
use crate::models::v2_cctp::{
    is_valid_tx_hash, parse_transfer_id, CctpPrepareBurnResponse, CctpPrepareMintResponse,
    CctpQuoteRequest, CctpQuoteResponse, CctpReattestResponse, CctpSubmitBurnRequest,
    CctpSubmitBurnResponse, CctpSubmitMintRequest, CctpSubmitMintResponse,
    CctpTransferStatusResponse, CctpValidationError,
};
use crate::models::ApiResponse;
use crate::state::AppState;

fn map_validation(err: CctpValidationError) -> ApiError {
    match err {
        CctpValidationError::UnsupportedCorridor => ApiError::UnsupportedCorridor,
        CctpValidationError::InvalidFinality => ApiError::InvalidFinality,
        CctpValidationError::InvalidRecipient => ApiError::InvalidRecipient,
        CctpValidationError::InvalidAmount => {
            ApiError::InvalidAmount("amount must be a positive decimal string".to_string())
        }
        CctpValidationError::InvalidSender => ApiError::Validation(
            "sender must be a valid G-address for Stellar or 0x address for EVM source".to_string(),
        ),
        CctpValidationError::InvalidMintSubmitter => ApiError::Validation(
            "mint_submitter must be a valid Stellar G-address for evm_to_stellar".to_string(),
        ),
        CctpValidationError::StellarRemainder => ApiError::Validation(
            "Stellar outbound amount must have zero 7th-decimal remainder".to_string(),
        ),
    }
}

fn parse_transfer_id_param(transfer_id: &str) -> Result<()> {
    parse_transfer_id(transfer_id)
        .map(|_| ())
        .map_err(ApiError::Validation)
}

fn validate_submit_tx_hash(tx_hash: &str) -> Result<()> {
    if is_valid_tx_hash(tx_hash) {
        Ok(())
    } else {
        Err(ApiError::Validation(
            "tx_hash must be a 64-hex Stellar hash or 0x-prefixed 32-byte EVM hash".to_string(),
        ))
    }
}

fn cctp_not_enabled() -> ApiError {
    ApiError::CctpNotEnabled(
        "Circle CCTP bridge settlement is not enabled on this deployment".to_string(),
    )
}

/// `POST /api/v2/bridge/cctp/quote`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/quote",
    tag = "cctp",
    request_body = CctpQuoteRequest,
    responses(
        (status = 200, description = "CCTP fee quote (disabled until backend is enabled)", body = CctpQuoteResponse),
        (status = 400, description = "Invalid request (e.g. stellar source with fast finality)"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_quote(
    _state: State<Arc<AppState>>,
    request_id: RequestId,
    Json(body): Json<CctpQuoteRequest>,
) -> Result<Json<ApiResponse<CctpQuoteResponse>>> {
    body.validate().map_err(map_validation)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/prepare-burn`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/prepare-burn",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Prepared burn wallet payload", body = CctpPrepareBurnResponse),
        (status = 400, description = "Invalid transfer ID"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_prepare_burn(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<CctpPrepareBurnResponse>>> {
    parse_transfer_id_param(&transfer_id)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/submit-burn`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/submit-burn",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    request_body = CctpSubmitBurnRequest,
    responses(
        (status = 200, description = "Burn tx hash recorded", body = CctpSubmitBurnResponse),
        (status = 400, description = "Invalid transfer ID or tx_hash"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_submit_burn(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
    Json(body): Json<CctpSubmitBurnRequest>,
) -> Result<Json<ApiResponse<CctpSubmitBurnResponse>>> {
    parse_transfer_id_param(&transfer_id)?;
    validate_submit_tx_hash(&body.tx_hash)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}

/// `GET /api/v2/bridge/cctp/{transfer_id}`
#[utoipa::path(
    get,
    path = "/api/v2/bridge/cctp/{transfer_id}",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Transfer saga status", body = CctpTransferStatusResponse),
        (status = 400, description = "Invalid transfer ID"),
        (status = 404, description = "Transfer not found"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_get_transfer(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<CctpTransferStatusResponse>>> {
    parse_transfer_id_param(&transfer_id)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/prepare-mint`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/prepare-mint",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Prepared mint wallet payload", body = CctpPrepareMintResponse),
        (status = 400, description = "Invalid transfer ID"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_prepare_mint(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<CctpPrepareMintResponse>>> {
    parse_transfer_id_param(&transfer_id)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/submit-mint`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/submit-mint",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    request_body = CctpSubmitMintRequest,
    responses(
        (status = 200, description = "Mint tx hash recorded", body = CctpSubmitMintResponse),
        (status = 400, description = "Invalid transfer ID or tx_hash"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_submit_mint(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
    Json(body): Json<CctpSubmitMintRequest>,
) -> Result<Json<ApiResponse<CctpSubmitMintResponse>>> {
    parse_transfer_id_param(&transfer_id)?;
    validate_submit_tx_hash(&body.tx_hash)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/reattest`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/reattest",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Attestation re-poll requested", body = CctpReattestResponse),
        (status = 400, description = "Invalid transfer ID"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_reattest(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<CctpReattestResponse>>> {
    parse_transfer_id_param(&transfer_id)?;
    let _ = request_id;
    Err(cctp_not_enabled())
}
