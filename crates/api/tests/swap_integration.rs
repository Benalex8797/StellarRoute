//! Integration tests for the swap prepare/submit endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use stellarroute_api::{
    broadcast::MockTransactionBroadcaster,
    state::DatabasePools,
    swap::store::InMemorySwapQuoteStore,
    AppState,
};
use tower::ServiceExt;

const TEST_SENDER: &str = "GCKFBEIYTKP6RQBULIFBXLEMTUFEHYZU7YMGC3JMJLXZA65LRA7SNLQ3";

async fn setup_router() -> axum::Router {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://localhost/unused")
        .expect("failed to create lazy pool");

    let app_state = AppState::new(DatabasePools::new(pool, None)).with_swap_services(
        Arc::new(InMemorySwapQuoteStore::default()),
        Arc::new(MockTransactionBroadcaster::succeed("test-tx-hash")),
    );

    stellarroute_api::routes::create_router(app_state.into_arc())
}

fn valid_prepare_payload() -> Value {
    json!({
        "route": {
            "hops": [{
                "from_asset": { "asset_code": "native", "asset_issuer": null },
                "to_asset": { "asset_code": "USDC", "asset_issuer": null },
                "source": "sdex",
                "fee_bps": 30,
                "price": "0.12",
                "venue_ref": "sdex-venue"
            }]
        },
        "amount": "100",
        "sender": TEST_SENDER,
    })
}

async fn post(router: axum::Router, path: &str, payload: &Value) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("request failed");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn prepare_swap_valid_request_returns_prepared_quote() {
    let router = setup_router().await;
    let (status, body) = post(router, "/api/v1/swap/prepare", &valid_prepare_payload()).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["quote_id"].is_string());
    assert!(body["data"]["xdr_envelope"].is_string());
    assert!(body["data"]["expected_output"].is_string());
}

#[tokio::test]
async fn prepare_swap_rejects_zero_amount() {
    let mut payload = valid_prepare_payload();
    payload["amount"] = json!("0");
    let router = setup_router().await;
    let (status, body) = post(router, "/api/v1/swap/prepare", &payload).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["data"]["error"], "validation_error");
}

#[tokio::test]
async fn prepare_swap_rejects_empty_route() {
    let mut payload = valid_prepare_payload();
    payload["route"]["hops"] = json!([]);
    let router = setup_router().await;
    let (status, body) = post(router, "/api/v1/swap/prepare", &payload).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["data"]["error"], "validation_error");
}

#[tokio::test]
async fn submit_swap_requires_quote_id_and_signed_xdr() {
    let payload = json!({ "quote_id": "", "signed_xdr": "AAAAAgAAAAA=" });
    let router = setup_router().await;
    let (status, body) = post(router, "/api/v1/swap/submit", &payload).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["data"]["error"], "validation_error");
}

#[tokio::test]
async fn submit_swap_rejects_empty_signed_xdr() {
    let payload = json!({ "quote_id": "q-1", "signed_xdr": "" });
    let router = setup_router().await;
    let (status, body) = post(router, "/api/v1/swap/submit", &payload).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["data"]["error"], "validation_error");
}
