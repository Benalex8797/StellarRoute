//! OpenAPI documentation

use utoipa::OpenApi;

use crate::models::{
    AssetInfo, AssetMetadataBulkResponse, AssetMetadataResponse, BatchItemError,
    BatchQuoteItemResult, BatchQuoteResponse, CacheFlushResponse, CacheMetricsResponse,
    DataFreshness, DependenciesHealthResponse, ErrorResponse, ExcludedVenueInfo,
    ExclusionDiagnostics, ExclusionReason, HealthResponse, OrderbookLevel, OrderbookResponse,
    PairsResponse, PathStep, PriceHistoryPoint, PriceHistoryResponse,
    QuoteExpirationWebhookPayload, QuoteExpirationWebhookRegistrationResponse,
    QuoteRationaleMetadata, QuoteResponse, RouteResponse, TradingPair, VenueEvaluation,
};

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::health::health_check,
        crate::routes::health::dependency_health,
        crate::routes::metrics::cache_metrics,
        crate::routes::assets::get_asset_metadata,
        crate::routes::assets::list_assets_metadata,
        crate::routes::activity::list_swap_activity,
        crate::routes::pairs::list_pairs,
        crate::routes::pairs::list_markets,
        crate::routes::orderbook::get_orderbook,
        crate::routes::orderbook::get_batch_orderbooks,
        crate::routes::price_history::get_price_history,
        crate::routes::quote::get_quote,
        crate::routes::quote::get_route,
        crate::routes::admin::flush_cache,
        crate::routes::quote::get_batch_quotes,
        crate::routes::integrator_webhooks::upsert_quote_expiration_webhook,
        crate::routes::kill_switch::get_kill_switch,
        crate::routes::kill_switch::update_kill_switch,
        crate::routes::swap::prepare_swap,
        crate::routes::swap::submit_swap,
        crate::routes::v2::api_v2_info,
        crate::routes::v2::canonicalize_asset,
    ),
    components(schemas(
        HealthResponse,
        DependenciesHealthResponse,
        CacheMetricsResponse,
        CacheFlushResponse,
        AssetMetadataResponse,
        AssetMetadataBulkResponse,
        crate::routes::activity::SwapActivityItem,
        crate::routes::activity::SwapActivityResponse,
        PairsResponse,
        TradingPair,
        AssetInfo,
        OrderbookResponse,
        OrderbookLevel,
        crate::models::OrderbookSummary,
        QuoteResponse,
        PriceHistoryPoint,
        PriceHistoryResponse,
        RouteResponse,
        QuoteRationaleMetadata,
        VenueEvaluation,
        PathStep,
        DataFreshness,
        ExclusionDiagnostics,
        ExcludedVenueInfo,
        ExclusionReason,
        BatchQuoteResponse,
        BatchQuoteItemResult,
        BatchItemError,
        QuoteExpirationWebhookRegistrationResponse,
        QuoteExpirationWebhookPayload,
        ErrorResponse,
        crate::models::request::BatchQuoteRequest,
        crate::models::request::QuoteRequestItem,
        crate::models::request::BatchOrderbookRequest,
        crate::models::request::OrderbookRequestItem,
        crate::models::response::BatchOrderbookResponse,
        crate::models::response::BatchOrderbookItemResult,
        crate::models::request::QuoteType,
        crate::models::request::QuoteExpirationWebhookRegistrationRequest,
        crate::models::request::AssetPath,
        crate::models::request::SwapPrepareRequest,
        crate::models::request::SwapSubmitRequest,
        crate::models::response::SwapPrepareResponse,
        crate::models::response::SwapSubmitResponse,
        crate::routes::simulation_route::RouteDryRunPath,
        crate::routes::simulation_route::RouteDryRunHop,
        crate::models::v2::ApiV2Info,
        crate::models::v2::ChainAssetV2,
        crate::models::v2::CanonicalizeAssetRequest,
        crate::models::v2::CanonicalizeAssetResponse,
    )),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "assets", description = "Asset metadata endpoints"),
        (name = "activity", description = "Indexed contract activity endpoints"),
        (name = "trading", description = "Trading and market data endpoints"),
        (name = "admin", description = "Administrative endpoints"),
        (name = "integrator", description = "Integrator configuration and webhook endpoints"),
        (name = "swap", description = "Live swap path: prepare an unsigned transaction from a route, then submit a signed transaction on-chain"),
        (name = "v2", description = "API v2 seam: chain-aware asset canonicalize/info only (no v2 quotes). Bridges are metadata-only/non-executable; provider controls apply only to provider-tagged candidates."),
    ),
    info(
        title = "StellarRoute API",
        version = "0.1.0",
        description = "REST API for DEX aggregation on Stellar Network. \
            Clients may send an optional X-Request-ID header for support correlation; \
            the API echoes the same header in every response. Large quote responses \
            honor `Accept-Encoding: br` or `Accept-Encoding: gzip`; smaller quote \
            responses are left uncompressed, and Prometheus metrics track original \
            versus wire response bytes.\n\n\
            ## Batch Quotes\n\
            `POST /api/v1/batch/quote` evaluates up to 25 trading pairs concurrently \
            against a shared market snapshot. Per-item failures do not abort the batch.",
        contact(
            name = "StellarRoute",
            url = "https://github.com/stellarroute/stellarroute"
        ),
        license(
            name = "MIT",
        ),
    ),
)]
pub struct ApiDoc;
