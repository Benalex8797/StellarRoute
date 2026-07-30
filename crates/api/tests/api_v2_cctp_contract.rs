//! Contract tests for Circle CCTP v2 bridge API surface (contract-freeze only).
//!
//! Verifies path registration, OpenAPI documentation, backward-compatible
//! `GET /api/v2`, fail-closed typed `503 cctp_not_enabled` responses, JSON
//! snake_case wire shapes, unknown-field rejection, and Stellar-source fast
//! finality validation — without a live bridge backend.

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use stellarroute_api::{state::DatabasePools, Server, ServerConfig};
use tower::ServiceExt;

async fn setup_test_router() -> axum::Router {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://localhost/unused")
        .expect("Failed to create lazy pool");

    Server::new(ServerConfig::default(), DatabasePools::new(pool, None))
        .await
        .into_router()
}

fn stellar_usdc() -> Value {
    json!({
        "chain_id": "stellar:testnet",
        "asset": "erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
        "canonical": "stellar:testnet/erc20:CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
        "symbol": "USDC"
    })
}

fn sepolia_usdc() -> Value {
    json!({
        "chain_id": "eip155:11155111",
        "asset": "erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238",
        "canonical": "eip155:11155111/erc20:0x1c7d4b196cb0c7b01d743fbc6116a902379c7238",
        "symbol": "USDC"
    })
}

fn sample_quote_body(direction: &str, finality: &str) -> Value {
    let (source_chain, dest_chain, source_asset, dest_asset) = if direction == "stellar_to_evm" {
        (
            "stellar:testnet",
            "eip155:11155111",
            stellar_usdc(),
            sepolia_usdc(),
        )
    } else {
        (
            "eip155:11155111",
            "stellar:testnet",
            sepolia_usdc(),
            stellar_usdc(),
        )
    };

    json!({
        "corridor_id": "circle-cctp:usdc:stellar-testnet:ethereum-sepolia",
        "provider": "circle-cctp",
        "direction": direction,
        "source_chain_id": source_chain,
        "destination_chain_id": dest_chain,
        "source_asset": source_asset,
        "destination_asset": dest_asset,
        "amount": "100.000000",
        "recipient": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0",
        "finality": finality
    })
}

async fn post_json(router: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, json)
}

async fn get_json(router: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, json)
}

fn assert_cctp_not_enabled(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["data"]["error"], "cctp_not_enabled");
}

#[tokio::test]
async fn api_v2_info_backward_compatible_with_empty_supported_corridors() {
    let router = setup_test_router().await;
    let (status, body) = get_json(&router, "/api/v2").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["v"], 2);
    assert_eq!(body["data"]["version"], 2);
    assert_eq!(body["data"]["bridge_settlement_executable"], false);
    assert_eq!(body["data"]["bridge_venues_metadata_only"], true);
    assert!(body["data"]["chain_aware_assets"].as_bool().unwrap());
    assert!(body["data"]["supported_chain_namespaces"].is_array());
    assert!(
        body["data"]["supported_corridors"].is_array(),
        "supported_corridors must be present"
    );
    assert_eq!(
        body["data"]["supported_corridors"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "supported_corridors must be empty on contract-freeze branch"
    );
}

#[tokio::test]
async fn cctp_quote_rejects_stellar_source_fast_finality_before_not_enabled() {
    let router = setup_test_router().await;
    let (status, body) = post_json(
        &router,
        "/api/v2/bridge/cctp/quote",
        sample_quote_body("stellar_to_evm", "fast"),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["data"]["error"], "invalid_finality");
}

#[tokio::test]
async fn cctp_quote_fail_closed_when_valid() {
    let router = setup_test_router().await;
    let (status, body) = post_json(
        &router,
        "/api/v2/bridge/cctp/quote",
        sample_quote_body("stellar_to_evm", "standard"),
    )
    .await;

    assert_cctp_not_enabled(status, &body);
}

#[tokio::test]
async fn cctp_transfer_endpoints_fail_closed() {
    let router = setup_test_router().await;
    let transfer_id = "550e8400-e29b-41d4-a716-446655440000";

    let post_cases = [
        (
            format!("/api/v2/bridge/cctp/{transfer_id}/prepare-burn"),
            json!({}),
        ),
        (
            format!("/api/v2/bridge/cctp/{transfer_id}/submit-burn"),
            json!({ "tx_hash": "0xabc" }),
        ),
        (
            format!("/api/v2/bridge/cctp/{transfer_id}/prepare-mint"),
            json!({}),
        ),
        (
            format!("/api/v2/bridge/cctp/{transfer_id}/submit-mint"),
            json!({ "tx_hash": "0xdef" }),
        ),
        (
            format!("/api/v2/bridge/cctp/{transfer_id}/reattest"),
            json!({}),
        ),
    ];

    for (uri, body) in post_cases {
        let (status, json) = post_json(&router, &uri, body).await;
        assert_cctp_not_enabled(status, &json);
    }

    let (status, json) = get_json(&router, &format!("/api/v2/bridge/cctp/{transfer_id}")).await;
    assert_cctp_not_enabled(status, &json);
}

#[tokio::test]
async fn cctp_quote_rejects_unknown_fields() {
    let router = setup_test_router().await;
    let mut body = sample_quote_body("evm_to_stellar", "standard");
    body["unexpected_field"] = json!("nope");

    let (status, _) = post_json(&router, "/api/v2/bridge/cctp/quote", body).await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "unknown fields must be rejected, got {status}"
    );
}

#[tokio::test]
async fn openapi_documents_cctp_bridge_paths_and_schemas() {
    let router = setup_test_router().await;
    let (status, body) = get_json(&router, "/api-docs/openapi.json").await;
    assert_eq!(status, StatusCode::OK);

    let paths = [
        ("/api/v2/bridge/cctp/quote", "post"),
        ("/api/v2/bridge/cctp/{transfer_id}/prepare-burn", "post"),
        ("/api/v2/bridge/cctp/{transfer_id}/submit-burn", "post"),
        ("/api/v2/bridge/cctp/{transfer_id}", "get"),
        ("/api/v2/bridge/cctp/{transfer_id}/prepare-mint", "post"),
        ("/api/v2/bridge/cctp/{transfer_id}/submit-mint", "post"),
        ("/api/v2/bridge/cctp/{transfer_id}/reattest", "post"),
    ];

    for (path, method) in paths {
        let op = &body["paths"][path][method];
        assert!(
            !op.is_null(),
            "{method} {path} must be documented in OpenAPI"
        );
        let tags = op["tags"].as_array().expect("tags");
        assert!(
            tags.iter().any(|t| t == "cctp"),
            "{method} {path} must be tagged cctp"
        );
    }

    let schemas = &body["components"]["schemas"];
    for name in [
        "ApiV2Info",
        "SupportedCorridor",
        "CctpQuoteRequest",
        "CctpQuoteResponse",
        "CctpTransferStatusResponse",
        "CctpPrepareBurnResponse",
        "CctpSubmitBurnRequest",
        "CctpSubmitBurnResponse",
        "CctpPrepareMintResponse",
        "CctpSubmitMintRequest",
        "CctpSubmitMintResponse",
        "CctpReattestResponse",
        "PreparedWalletPayload",
        "CctpFeeQuote",
    ] {
        assert!(
            schemas[name].is_object(),
            "{name} schema must exist in components.schemas"
        );
    }

    let supported = &schemas["ApiV2Info"]["properties"]["supported_corridors"];
    assert!(
        supported.is_object(),
        "ApiV2Info must document supported_corridors"
    );
}

#[test]
fn cctp_wire_models_use_snake_case() {
    use stellarroute_api::models::v2_cctp::{
        CctpDirection, CctpFinality, CctpTransferStatus, PreparedWalletPayload,
    };

    let direction = serde_json::to_value(CctpDirection::StellarToEvm).unwrap();
    assert_eq!(direction, "stellar_to_evm");

    let finality = serde_json::to_value(CctpFinality::Standard).unwrap();
    assert_eq!(finality, "standard");

    let status = serde_json::to_value(CctpTransferStatus::AwaitingAttestation).unwrap();
    assert_eq!(status, "awaiting_attestation");

    let payload = PreparedWalletPayload::StellarXdr {
        network_passphrase: "Test SDF Network ; September 2015".into(),
        xdr_envelope: "AAAA".into(),
    };
    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["type"], "stellar_xdr");
    assert!(json.get("network_passphrase").is_some());
    assert!(json.get("xdr_envelope").is_some());
}
