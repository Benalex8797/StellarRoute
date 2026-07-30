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
    CctpQuoteRequest, CctpSubmitBurnRequest, CctpSubmitMintRequest, CctpValidationError,
};
use crate::models::ApiResponse;
use crate::state::AppState;

fn map_validation(err: CctpValidationError) -> ApiError {
    match err {
        CctpValidationError::UnsupportedCorridor => ApiError::UnsupportedCorridor,
        CctpValidationError::InvalidFinality => ApiError::InvalidFinality,
        CctpValidationError::InvalidRecipient => ApiError::InvalidRecipient,
        CctpValidationError::InvalidAmount => {
            ApiError::InvalidAmount("amount must be a non-empty decimal string".to_string())
        }
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
        (status = 200, description = "CCTP fee quote (disabled until backend is enabled)", body = crate::models::v2_cctp::CctpQuoteResponse),
        (status = 400, description = "Invalid request (e.g. stellar source with fast finality)"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_quote(
    _state: State<Arc<AppState>>,
    request_id: RequestId,
    Json(body): Json<CctpQuoteRequest>,
) -> Result<Json<ApiResponse<()>>> {
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
        (status = 200, description = "Prepared burn wallet payload", body = crate::models::v2_cctp::CctpPrepareBurnResponse),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_prepare_burn(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<()>>> {
    let _ = (transfer_id, request_id);
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
        (status = 200, description = "Burn tx hash recorded", body = crate::models::v2_cctp::CctpSubmitBurnResponse),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_submit_burn(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
    Json(body): Json<CctpSubmitBurnRequest>,
) -> Result<Json<ApiResponse<()>>> {
    if body.tx_hash.trim().is_empty() {
        return Err(ApiError::Validation(
            "tx_hash is required for burn acknowledgement".to_string(),
        ));
    }
    let _ = (transfer_id, request_id);
    Err(cctp_not_enabled())
}

/// `GET /api/v2/bridge/cctp/{transfer_id}`
#[utoipa::path(
    get,
    path = "/api/v2/bridge/cctp/{transfer_id}",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Transfer saga status", body = crate::models::v2_cctp::CctpTransferStatusResponse),
        (status = 404, description = "Transfer not found"),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_get_transfer(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<()>>> {
    let _ = (transfer_id, request_id);
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/prepare-mint`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/prepare-mint",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Prepared mint wallet payload", body = crate::models::v2_cctp::CctpPrepareMintResponse),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_prepare_mint(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<()>>> {
    let _ = (transfer_id, request_id);
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
        (status = 200, description = "Mint tx hash recorded", body = crate::models::v2_cctp::CctpSubmitMintResponse),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_submit_mint(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
    Json(body): Json<CctpSubmitMintRequest>,
) -> Result<Json<ApiResponse<()>>> {
    if body.tx_hash.trim().is_empty() {
        return Err(ApiError::Validation(
            "tx_hash is required for mint acknowledgement".to_string(),
        ));
    }
    let _ = (transfer_id, request_id);
    Err(cctp_not_enabled())
}

/// `POST /api/v2/bridge/cctp/{transfer_id}/reattest`
#[utoipa::path(
    post,
    path = "/api/v2/bridge/cctp/{transfer_id}/reattest",
    tag = "cctp",
    params(("transfer_id" = String, Path, description = "Transfer UUID")),
    responses(
        (status = 200, description = "Attestation re-poll requested", body = crate::models::v2_cctp::CctpReattestResponse),
        (status = 503, description = "CCTP bridge not enabled"),
    )
)]
pub async fn cctp_reattest(
    _state: State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    request_id: RequestId,
) -> Result<Json<ApiResponse<()>>> {
    let _ = (transfer_id, request_id);
    Err(cctp_not_enabled())
}
