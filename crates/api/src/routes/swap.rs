//! Swap prepare/submit HTTP handlers.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{Duration, Utc};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::{
    audit::{AuditRedactor, SwapSubmitOutcome},
    broadcast::BroadcastError,
    error::{ApiError, Result},
    metrics::{record_swap_prepare, record_swap_submit, swap_inflight_dec, swap_inflight_inc},
    models::{
        request::SwapPrepareRequest, ApiResponse, SwapPrepareResponse, SwapSubmitRequest,
        SwapSubmitResponse,
    },
    routes::simulation_route::request_route_to_swap_path,
    state::AppState,
    swap::store::{
        hash_xdr, ClaimSubmitOutcome, PreparedSwapQuote, SubmissionStatus, SwapStoreError,
    },
};

const DEFAULT_PREPARE_TTL_SECS: i64 = 120;

/// POST /api/v1/swap/prepare
#[utoipa::path(
    post,
    path = "/api/v1/swap/prepare",
    tag = "swap",
    request_body(content = SwapPrepareRequest, description = "Build an unsigned swap transaction"),
    responses(
        (status = 200, description = "Unsigned transaction envelope", body = ApiResponse<SwapPrepareResponse>),
        (status = 400, description = "Validation error", body = crate::models::ErrorResponse),
        (status = 404, description = "Route not executable", body = crate::models::ErrorResponse),
        (status = 422, description = "Stale market data", body = crate::models::ErrorResponse),
    )
)]
pub async fn prepare_swap(
    State(state): State<Arc<AppState>>,
    request_id: crate::middleware::RequestId,
    Json(body): Json<SwapPrepareRequest>,
) -> Result<impl IntoResponse> {
    swap_inflight_inc("prepare");
    let started = Instant::now();
    let trace_id = String::new();

    let result = prepare_swap_inner(&state, &body).await;

    let elapsed = started.elapsed();
    match &result {
        Ok(_) => record_swap_prepare(elapsed, "none"),
        Err(e) => record_swap_prepare(elapsed, prepare_error_class(e)),
    }
    swap_inflight_dec("prepare");

    let prepared = result?;

    state.swap_submit_audit_writer.emit_swap_submit(
        &prepared.quote_id,
        None::<String>,
        &body.sender,
        request_id.as_str(),
        &trace_id,
        started.elapsed().as_millis() as u64,
        SwapSubmitOutcome::Prepared,
        "none",
        serde_json::json!({ "expected_output": prepared.expected_output }),
    );

    Ok((
        StatusCode::OK,
        Json(ApiResponse::new(prepared, request_id.as_str())),
    ))
}

async fn prepare_swap_inner(
    state: &AppState,
    body: &SwapPrepareRequest,
) -> Result<SwapPrepareResponse> {
    validate_stellar_account(&body.sender)?;

    if body.route.hops.is_empty() {
        return Err(ApiError::Validation(
            "route.hops must contain at least one hop".to_string(),
        ));
    }

    let amount: f64 = body
        .amount
        .parse()
        .map_err(|_| ApiError::Validation("amount must be a valid number".to_string()))?;
    if !amount.is_finite() || amount <= 0.0 {
        return Err(ApiError::Validation(
            "amount must be greater than zero".to_string(),
        ));
    }

    request_route_to_swap_path(&body.route)?;

    let quote_id = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::seconds(DEFAULT_PREPARE_TTL_SECS);
    let unsigned_payload = format!("SR-PREPARE:{quote_id}:{}", body.sender.trim());
    let xdr_envelope =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, unsigned_payload);
    let unsigned_hash = hash_xdr(&xdr_envelope);

    let expected_output = body
        .min_output
        .clone()
        .unwrap_or_else(|| format!("{:.7}", amount * 0.995));

    let prepared = PreparedSwapQuote {
        quote_id: quote_id.clone(),
        sender_account_hash: AuditRedactor::redact_account(body.sender.trim()),
        unsigned_xdr_hash: unsigned_hash,
        expires_at,
        estimated_output: expected_output.clone(),
        min_output: expected_output.clone(),
        valid_until_ledger: None,
        submission_status: SubmissionStatus::Prepared,
        tx_hash: None,
    };

    state
        .swap_quote_store
        .insert_prepared(&prepared)
        .await
        .map_err(map_store_error)?;

    Ok(SwapPrepareResponse {
        quote_id,
        xdr_envelope,
        expected_output,
        min_output: Some(prepared.min_output),
        expires_at: expires_at.timestamp_millis(),
    })
}

fn prepare_error_class(err: &ApiError) -> &'static str {
    match err {
        ApiError::Validation(_) | ApiError::InvalidAsset(_) | ApiError::InvalidAmount(_) => {
            "validation"
        }
        ApiError::NoRouteFound => "simulation_failed",
        ApiError::NotExecutable(_) => "simulation_failed",
        ApiError::StaleMarketData { .. } => "quote_expired",
        _ => "internal",
    }
}

/// POST /api/v1/swap/submit
#[utoipa::path(
    post,
    path = "/api/v1/swap/submit",
    tag = "swap",
    request_body(content = SwapSubmitRequest, description = "Broadcast a signed swap transaction"),
    responses(
        (status = 200, description = "Transaction accepted by the network", body = ApiResponse<SwapSubmitResponse>),
        (status = 202, description = "Transaction pending", body = ApiResponse<SwapSubmitResponse>),
        (status = 404, description = "Unknown quote_id", body = crate::models::ErrorResponse),
        (status = 409, description = "Quote already submitted (idempotent conflict)", body = crate::models::ErrorResponse),
        (status = 422, description = "Quote expired", body = crate::models::ErrorResponse),
        (status = 400, description = "Validation error", body = crate::models::ErrorResponse),
    )
)]
pub async fn submit_swap(
    State(state): State<Arc<AppState>>,
    request_id: crate::middleware::RequestId,
    Json(body): Json<SwapSubmitRequest>,
) -> Result<impl IntoResponse> {
    swap_inflight_inc("submit");
    let started = Instant::now();
    let trace_id = String::new();

    let outcome = submit_swap_inner(&state, &body).await;
    let elapsed = started.elapsed();

    match &outcome {
        Ok((response, status)) => {
            record_swap_submit(elapsed, "none");
            state.swap_submit_audit_writer.emit_swap_submit(
                &body.quote_id,
                Some(&response.tx_hash),
                "unknown",
                request_id.as_str(),
                &trace_id,
                elapsed.as_millis() as u64,
                SwapSubmitOutcome::Submitted,
                "none",
                serde_json::json!({ "status": response.status }),
            );
            swap_inflight_dec("submit");
            return Ok((
                *status,
                Json(ApiResponse::new(response.clone(), request_id.as_str())),
            ));
        }
        Err(e) => {
            record_swap_submit(elapsed, submit_error_class(e));
            if let ApiError::QuoteExpired { quote_id } = e {
                state.swap_submit_audit_writer.emit_swap_submit(
                    quote_id,
                    None::<String>,
                    "unknown",
                    request_id.as_str(),
                    &trace_id,
                    elapsed.as_millis() as u64,
                    SwapSubmitOutcome::Failed,
                    "quote_expired",
                    serde_json::json!({}),
                );
            }
            swap_inflight_dec("submit");
        }
    }

    outcome.map(|(response, status)| {
        (
            status,
            Json(ApiResponse::new(response, request_id.as_str())),
        )
    })
}

async fn submit_swap_inner(
    state: &AppState,
    body: &SwapSubmitRequest,
) -> Result<(SwapSubmitResponse, StatusCode)> {
    if body.quote_id.trim().is_empty() {
        return Err(ApiError::Validation("quote_id is required".to_string()));
    }
    if body.signed_xdr.trim().is_empty() {
        return Err(ApiError::Validation("signed_xdr is required".to_string()));
    }
    if base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        body.signed_xdr.trim(),
    )
    .is_err()
    {
        return Err(ApiError::Validation(
            "signed_xdr must be valid base64".to_string(),
        ));
    }

    let quote = state
        .swap_quote_store
        .get(body.quote_id.trim())
        .await
        .map_err(map_store_error)?
        .ok_or_else(|| ApiError::QuoteNotFound {
            quote_id: body.quote_id.clone(),
        })?;

    if quote.submission_status == SubmissionStatus::Submitted {
        let tx_hash = quote.tx_hash.unwrap_or_default();
        return Err(ApiError::Conflict {
            message: "Quote has already been submitted".to_string(),
            quote_id: body.quote_id.clone(),
            tx_hash,
            status: "already_submitted".to_string(),
        });
    }

    if Utc::now() > quote.expires_at {
        let _ = state.swap_quote_store.mark_failed(body.quote_id.trim()).await;
        return Err(ApiError::QuoteExpired {
            quote_id: body.quote_id.clone(),
        });
    }

    let signed_hash = hash_xdr(body.signed_xdr.trim());
    if signed_hash == quote.unsigned_xdr_hash {
        return Err(ApiError::Validation(
            "signed_xdr must differ from the unsigned prepare envelope".to_string(),
        ));
    }

    let claimed = state
        .swap_quote_store
        .claim_for_submit(body.quote_id.trim())
        .await
        .map_err(|e| map_store_error_for_quote(e, body.quote_id.trim()))?;

    let quote = match claimed {
        ClaimSubmitOutcome::Claimed(quote) => quote,
        ClaimSubmitOutcome::AlreadySubmitted { tx_hash } => {
            return Err(ApiError::Conflict {
                message: "Quote has already been submitted".to_string(),
                quote_id: body.quote_id.clone(),
                tx_hash,
                status: "already_submitted".to_string(),
            });
        }
        ClaimSubmitOutcome::InProgress => {
            return Err(ApiError::Conflict {
                message: "Quote submission is already in progress".to_string(),
                quote_id: body.quote_id.clone(),
                tx_hash: String::new(),
                status: "in_progress".to_string(),
            });
        }
    };

    let broadcast = match state
        .transaction_broadcaster
        .submit(body.signed_xdr.trim())
        .await
    {
        Ok(result) => result,
        Err(err) => {
            let _ = state.swap_quote_store.mark_failed(body.quote_id.trim()).await;
            return Err(map_broadcast_error(err));
        }
    };

    state
        .swap_quote_store
        .finalize_submit(body.quote_id.trim(), &broadcast.tx_hash)
        .await
        .map_err(|e| map_store_error_for_quote(e, body.quote_id.trim()))?;

    let http_status = if broadcast.status == "success" {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };

    Ok((
        SwapSubmitResponse {
            quote_id: body.quote_id.clone(),
            tx_hash: broadcast.tx_hash,
            status: broadcast.status,
            output_amount: Some(quote.estimated_output),
            ledger: broadcast.ledger,
        },
        http_status,
    ))
}

fn map_store_error(err: SwapStoreError) -> ApiError {
    match err {
        SwapStoreError::NotFound => ApiError::QuoteNotFound {
            quote_id: String::new(),
        },
        SwapStoreError::Database(e) => ApiError::Internal(Arc::new(anyhow::anyhow!(e))),
    }
}

fn map_store_error_for_quote(err: SwapStoreError, quote_id: &str) -> ApiError {
    match err {
        SwapStoreError::NotFound => ApiError::QuoteNotFound {
            quote_id: quote_id.to_string(),
        },
        other => map_store_error(other),
    }
}

fn map_broadcast_error(err: BroadcastError) -> ApiError {
    match err {
        BroadcastError::Validation(msg) => ApiError::Validation(msg),
        BroadcastError::Timeout => ApiError::DependencyUnavailable(
            "Horizon timed out while submitting transaction".to_string(),
        ),
        BroadcastError::InsufficientFee => ApiError::Validation(
            "Transaction fee is insufficient for network submission".to_string(),
        ),
        BroadcastError::InsufficientBalance => ApiError::Validation(
            "Source account has insufficient balance for this swap".to_string(),
        ),
        BroadcastError::SlippageExceeded => {
            ApiError::NotExecutable("On-chain execution would exceed slippage bounds".to_string())
        }
        BroadcastError::BadSignature => {
            ApiError::Validation("Transaction signature is invalid".to_string())
        }
        BroadcastError::RpcError(msg) => ApiError::DependencyUnavailable(msg),
    }
}

fn submit_error_class(err: &ApiError) -> &'static str {
    match err {
        ApiError::QuoteExpired { .. } => "quote_expired",
        ApiError::QuoteNotFound { .. } => "quote_not_found",
        ApiError::Conflict { .. } => "duplicate_quote",
        ApiError::Validation(_) => "validation",
        ApiError::DependencyUnavailable(_) => "rpc_error",
        _ => "internal",
    }
}

fn validate_stellar_account(sender: &str) -> Result<()> {
    let sender = sender.trim();
    if sender.len() != 56 || !sender.starts_with('G') {
        return Err(ApiError::Validation(
            "sender must be a valid Stellar G-address (56 characters)".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_stellar_account_rejects_short_keys() {
        assert!(validate_stellar_account("GSHORT").is_err());
    }
}
