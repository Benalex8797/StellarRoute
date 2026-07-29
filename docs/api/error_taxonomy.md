# API Error Taxonomy

This document defines the standard error taxonomy for the StellarRoute API.

## Error Response Format

All API errors return a consistent JSON body:

```json
{
  "error": "error_code",
  "message": "Human-readable description",
  "details": { ... }
}
```

- `error`: A machine-readable string code in `snake_case`.
- `message`: A descriptive message for developers/users.
- `details`: (Optional) Structured context about the failure (e.g., validation rules, stale counts).

## Error Catalog

| Code | HTTP Status | Description |
|:-----|:------------|:------------|
| `bad_request` | 400 | The request is malformed or contains invalid parameters. |
| `invalid_asset` | 400 | One of the asset identifiers in the request is invalid. |
| `invalid_amount` | 400 | The requested amount is invalid (e.g. non-numeric, zero, or negative). |
| `invalid_slippage` | 400 | The requested slippage tolerance is invalid. |
| `invalid_asset_format` | 400 | An asset identifier is malformed (wrong shape, not a parse failure of the value itself). |
| `validation_error` | 400 | The request parameters failed validation (e.g. amount <= 0). |
| `unauthorized` | 401 | The request lacks valid authentication credentials. |
| `not_found` | 404 | The requested resource (pair, orderbook, etc.) was not found. |
| `no_route` | 404 | No trading route was found for the given pair. |
| `stale_market_data` | 422 | The quote could not be generated because the underlying market data is too stale. |
| `not_executable` | 422 | The route would fail execution on-chain (simulation detected failure). |
| `quote_expired` | 422 | The referenced prepare quote has expired; call `prepare` again before submitting. |
| `rate_limit_exceeded` | 429 | Too many requests have been made in a short period. |
| `internal_error` | 500 | An unexpected error occurred on the server. |
| `not_implemented` | 501 | The requested operation is part of the documented API contract but not yet available. |
| `overloaded` | 503 | The server is currently processing too many requests. |
| `quote_not_found` | 404 | The referenced `quote_id` is unknown or no longer valid. |
| `duplicate_quote` | 409 | The prepare quote was already submitted or is currently being submitted. |

This table is the canonical list backing `crates/api/src/models/response.rs`'s
`ApiErrorCode::ALL` and sdk-js's `API_ERROR_CODES` (`sdk-js/src/types.ts`).
`crates/api/tests/openapi_swap_contract.rs` fails the build if any of the
three drift apart — update all three together when adding a new code.

## Swap prepare/submit

`POST /api/v1/swap/prepare` and `POST /api/v1/swap/submit` (tag `swap` in
Swagger UI) implement the live swap path: `prepare` validates a pre-selected
route and amount and returns an unsigned transaction envelope plus `quote_id`;
`submit` accepts a signed envelope and broadcasts it via Horizon with
idempotency keyed on `quote_id`. See
[`docs/readiness/live-swap-testnet-checklist.md`](../readiness/live-swap-testnet-checklist.md)
for the operational checklist.

## SDK Mapping

The JS SDK (`@stellarroute/sdk-js`) maps these codes to the `StellarRouteApiError` class.

| SDK Method | Logic |
|:-----------|:------|
| `isNotFound()` | `status === 404 \|\| code === 'not_found'` |
| `isRateLimited()` | `status === 429 \|\| code === 'rate_limit_exceeded'` |
| `isValidationError()` | `status === 400 \|\| ['validation_error', 'invalid_asset'].includes(code)` |

## WebSocket Errors

WebSocket endpoints use the same error codes as REST endpoints, plus additional WebSocket-specific codes:

| Code | Description |
|------|-------------|
| `unknown_action` | The `action` field in a client message is not recognized. |
| `invalid_subscription` | Subscription object is malformed or missing required fields. |
| `too_many_subscriptions` | Connection has reached the maximum subscriptions per connection limit. |

See [WebSocket Quote Stream API](websocket.md) for complete WebSocket protocol documentation and error handling guidance.

## Freshness SLO & Configuration Knobs

To prevent execution on stale rates, the StellarRoute API implements strict freshness checks:
- **SLO**: Market quotes/routes are rejected if the underlying data sources (SDEX offers or Soroban pool reserves) have not been updated within configured thresholds.
- **Error**: Rejections return HTTP 422 with the code `stale_market_data` containing details about the stale vs. fresh inputs.
- **Config Knobs**:
  - `freshness_threshold_secs.sdex`: Maximum age of SDEX offer data (default: 60s) before it is considered stale.
  - `freshness_threshold_secs.amm`: Maximum age of Soroban AMM state (default: 30s) before it is considered stale.
  - `staleness_threshold_secs`: Ultimate cutoff beyond which any source is rejected (default: 300s).

## Integration guidance

For practical retry semantics, backoff guidance, SDK helper examples, and frontend messaging recommendations, see [API Integrator Error Guide](integrator-error-guide.md).
