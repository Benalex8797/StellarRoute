//! Integration tests for POST /api/v1/swap/submit endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use stellarroute_api::{
    broadcast::{BroadcastError, MockTransactionBroadcaster},
    state::DatabasePools,
    swap::store::{InMemorySwapQuoteStore, PreparedSwapQuote, SubmissionStatus},
    AppState,
};
use tower::ServiceExt;

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn make_submit_router(
    store: Arc<InMemorySwapQuoteStore>,
    broadcaster: Arc<MockTransactionBroadcaster>,
) -> axum::Router {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://localhost/unused")
        .expect("Failed to create lazy pool");

    let app_state = AppState::new(DatabasePools::new(pool, None))
        .with_swap_services(store, broadcaster);

    // We use create_router directly to bypass Server::new which builds its own AppState
    stellarroute_api::routes::create_router(app_state.into_arc())
}

async fn post_submit(
    router: &axum::Router,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/swap/submit")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = router.clone().oneshot(req).await.expect("request failed");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, json)
}

fn sample_prepared_quote(id: &str, status: SubmissionStatus, expired: bool) -> PreparedSwapQuote {
    let expires_at = if expired {
        Utc::now() - Duration::minutes(5)
    } else {
        Utc::now() + Duration::minutes(5)
    };

    PreparedSwapQuote {
        quote_id: id.to_string(),
        sender_account_hash: "GABC...#abcd".to_string(),
        unsigned_xdr_hash: stellarroute_api::swap::store::hash_xdr("unsigned"),
        expires_at,
        estimated_output: "98".to_string(),
        min_output: "97".to_string(),
        valid_until_ledger: None,
        submission_status: status,
        tx_hash: if status == SubmissionStatus::Submitted {
            Some("existing-hash".to_string())
        } else {
            None
        },
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn submit_swap_success() {
    let store = Arc::new(InMemorySwapQuoteStore::default());
    stellarroute_api::swap::store::SwapQuoteStore::insert_prepared(
        store.as_ref(),
        &sample_prepared_quote("q-success", SubmissionStatus::Prepared, false),
    )
    .await
    .unwrap();

    let broadcaster = Arc::new(MockTransactionBroadcaster::succeed("new-tx-hash"));
    let router = make_submit_router(store.clone(), broadcaster).await;

    let body = json!({
        "quote_id": "q-success",
        "signed_xdr": "c2lnbmVk" // "signed" in base64
    });

    let (status, json) = post_submit(&router, body).await;
    
    assert_eq!(status, StatusCode::ACCEPTED, "mock broadcaster returns 'pending' status which maps to ACCEPTED");
    assert_eq!(json["data"]["tx_hash"], "new-tx-hash");
    assert_eq!(json["data"]["status"], "pending");
}

#[tokio::test]
async fn submit_swap_duplicate_conflict() {
    let store = Arc::new(InMemorySwapQuoteStore::default());
    stellarroute_api::swap::store::SwapQuoteStore::insert_prepared(
        store.as_ref(),
        &sample_prepared_quote("q-dup", SubmissionStatus::Submitted, false),
    )
    .await
    .unwrap();

    let broadcaster = Arc::new(MockTransactionBroadcaster::succeed("new-tx-hash"));
    let router = make_submit_router(store.clone(), broadcaster).await;

    let body = json!({
        "quote_id": "q-dup",
        "signed_xdr": "c2lnbmVk"
    });

    let (status, json) = post_submit(&router, body).await;
    
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["data"]["error"], "duplicate_quote");
}

#[tokio::test]
async fn submit_swap_unknown_quote() {
    let store = Arc::new(InMemorySwapQuoteStore::default());
    let broadcaster = Arc::new(MockTransactionBroadcaster::succeed("new-tx-hash"));
    let router = make_submit_router(store.clone(), broadcaster).await;

    let body = json!({
        "quote_id": "q-unknown",
        "signed_xdr": "c2lnbmVk"
    });

    let (status, json) = post_submit(&router, body).await;
    
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["data"]["error"], "quote_not_found");
}

#[tokio::test]
async fn submit_swap_expired_quote() {
    let store = Arc::new(InMemorySwapQuoteStore::default());
    stellarroute_api::swap::store::SwapQuoteStore::insert_prepared(
        store.as_ref(),
        &sample_prepared_quote("q-expired", SubmissionStatus::Prepared, true),
    )
    .await
    .unwrap();

    let broadcaster = Arc::new(MockTransactionBroadcaster::succeed("new-tx-hash"));
    let router = make_submit_router(store.clone(), broadcaster).await;

    let body = json!({
        "quote_id": "q-expired",
        "signed_xdr": "c2lnbmVk"
    });

    let (status, json) = post_submit(&router, body).await;
    
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY); // 422
    assert_eq!(json["data"]["error"], "quote_expired");
}

#[tokio::test]
async fn submit_swap_broadcast_failure() {
    let store = Arc::new(InMemorySwapQuoteStore::default());
    stellarroute_api::swap::store::SwapQuoteStore::insert_prepared(
        store.as_ref(),
        &sample_prepared_quote("q-fail", SubmissionStatus::Prepared, false),
    )
    .await
    .unwrap();

    let broadcaster = Arc::new(MockTransactionBroadcaster::fail(BroadcastError::InsufficientFee));
    let router = make_submit_router(store.clone(), broadcaster).await;

    let body = json!({
        "quote_id": "q-fail",
        "signed_xdr": "c2lnbmVk"
    });

    let (status, json) = post_submit(&router, body).await;
    
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["data"]["error"], "validation_error");
    assert!(json["data"]["message"].as_str().unwrap().contains("Transaction fee is insufficient"));
}
