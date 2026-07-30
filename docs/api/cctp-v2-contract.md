# Circle CCTP v2 bridge contract (testnet corridor freeze)

This document freezes the **additive** `/api/v2/bridge/cctp/*` API and SDK wire
contract for the first Circle CCTP corridor. A **backend core** (config, Postgres
saga store, Iris client, encoding, attestation/burn verifier seams, internal
service) exists on the feature branch but **public execution remains disabled**.

## Status (execution gate)

- Default: `CCTP_ENABLED=false` → all handlers return typed **`503 cctp_not_enabled`**.
- When explicitly enabled **and** direction-specific readiness passes (builders, verifiers,
  attestation, Iris, PG store, kill-switch), handlers execute the non-custodial saga.
- `GET /api/v2` lists corridor metadata with per-direction `executable` flags.
- Quote returns a one-time `access_token` (SHA-256 hash stored server-side). Mutations and
  status require header `x-cctp-transfer-access`.
- Quote supports `Idempotency-Key` (byte-identical replay returns cached response).
- `/api/v1/swap/*` remains classic Stellar XDR only and is unchanged.
- **Finality:** v1 corridor is **standard-only** both directions (`fast` rejected on wire).

Later backend work must gate executability via health/config — **not** by
hardcoding the default contract addresses below.

## First corridor (metadata only)

| Field | Stellar testnet | Ethereum Sepolia |
|-------|-----------------|------------------|
| CCTP domain | `27` | `0` |
| CAIP-2 chain id | `stellar:testnet` | `eip155:11155111` |
| Provider id | `circle-cctp` | `circle-cctp` |
| Corridor id | `circle-cctp:usdc:stellar-testnet:ethereum-sepolia` | (bidirectional) |

### Default contract identifiers (documentation / config contract)

**Stellar testnet**

- TokenMessengerMinter: `CDNG7HXAPBWICI2E3AUBP3YZWZELJLYSB6F5CC7WLDTLTHVM74SLRTHP`
- MessageTransmitter: `CBJ6MTCKKZG73PMDZCJMSFRD7DQEMI4FKDH7CGDSV4W6FHCRBCQAVVJY`
- CctpForwarder (inbound mint): `CA66Q2WFBND6V4UEB7RD4SAXSVIWMD6RA4X3U32ELVFGXV5PJK4T4VSZ`
- USDC Soroban token: `CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA`

**Ethereum Sepolia**

- TokenMessengerV2: `0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA`
- MessageTransmitterV2: `0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275`
- USDC: `0x1c7d4b196cb0c7b01d743fbc6116a902379c7238`

## Protocol facts (CCTP v2)

- **Outbound Stellar**: burn via Soroban `TokenMessengerMinter`.
- **Inbound Stellar**: mint via `CctpForwarder.mint_and_forward`.
- **EVM**: `TokenMessengerV2` / `MessageTransmitterV2`.
- **Burn is not idempotent**; **mint is idempotent**.
- Attestation polling is durable and must **never** trigger automatic re-burn.
- **Finality**: `standard` | `fast`; Stellar **source** burns must reject `fast`.

## Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v2/bridge/cctp/quote` | Fee quote + `transfer_id` (disabled) |
| `POST` | `/api/v2/bridge/cctp/{transfer_id}/prepare-burn` | Wallet burn payload |
| `POST` | `/api/v2/bridge/cctp/{transfer_id}/submit-burn` | Record source `tx_hash` only |
| `GET` | `/api/v2/bridge/cctp/{transfer_id}` | Saga status |
| `POST` | `/api/v2/bridge/cctp/{transfer_id}/prepare-mint` | Wallet mint payload |
| `POST` | `/api/v2/bridge/cctp/{transfer_id}/submit-mint` | Record destination `tx_hash` only |
| `POST` | `/api/v2/bridge/cctp/{transfer_id}/reattest` | Re-poll attestation |

### Recipient and sender constraints

- **Stellar `recipient` / optional `sender`**: Stellar account **G-address only** (ed25519 public key strkey). Muxed **M-addresses** and contract **C-addresses** are **not** accepted on quote validation.
- **EVM `recipient` / optional `sender`**: `0x`-prefixed 20-byte hex address (42 characters).
- Invalid optional `sender` returns generic `validation_error` (HTTP 400) before any fail-closed `cctp_not_enabled` response.

### Submit trust boundary

`submit-burn` and `submit-mint` accept **only** an on-chain transaction hash
acknowledgement (64-hex Stellar hash or `0x` + 64-hex EVM hash). Malformed,
empty, or unknown-field submit bodies return `validation_error` (HTTP 400).
Signed transaction broadcasting is the wallet/provider responsibility; the API
records the hash for attestation polling and later verification.

Malformed `{transfer_id}` path parameters also return `validation_error` before
any fail-closed `cctp_not_enabled` response.

### Saga states (`status` field)

`created`, `burn_prepared`, `burn_submitted`, `awaiting_attestation`,
`attestation_ready`, `mint_prepared`, `mint_submitted`, `completed`,
`attestation_failed`, `mint_failed_retryable`, `cancelled`, `provider_killed`.

HTTP error codes (`cctp_not_enabled`, `attestation_pending`, etc.) are
documented in [`error_taxonomy.md`](./error_taxonomy.md) and are distinct from
saga `status` values.

## Official sources

- [Stellar CCTP reference](https://developers.circle.com/cctp/references/stellar)
- [Stellar contracts](https://developers.circle.com/cctp/references/stellar-contracts)
- [Contract addresses](https://developers.circle.com/cctp/references/contract-addresses)
- [Supported chains and domains](https://developers.circle.com/cctp/concepts/supported-chains-and-domains)
- [Technical guide](https://developers.circle.com/cctp/references/technical-guide)
- [Finality and block confirmations](https://developers.circle.com/cctp/concepts/finality-and-block-confirmations)
- [Retry failed mint](https://developers.circle.com/cctp/howtos/retry-failed-mint)

## Access tokens and idempotency (HTTP gate)

### `x-cctp-transfer-access`

- Issued once on successful `POST /quote` (`access_token` in response body).
- Required on `GET /{transfer_id}` and every transfer mutation endpoint.
- **Persistence:** only `SHA-256` hex hash stored in `cctp_transfers.access_token_hash`.
- **Uniform 404:** missing transfer, missing header, malformed token, and wrong token all return
  `404 transfer_not_found` with the same envelope (no distinction that would leak existence).

### `Idempotency-Key` (quote only)

- Optional header, 1–128 UTF-8 characters (trimmed).
- Same key + byte-identical canonical JSON body → replay original `transfer_id` and access token.
- Same key + different body → `409` idempotency conflict (no extra transfer / Iris fee call).
- In-flight quote for another lease owner → `425`; retry with the same key until completed.

### `CCTP_ACCESS_TOKEN_HMAC_KEY`

Required when `CCTP_ENABLED=true`. Decoding (first match wins):

1. Hex (even length)
2. Base64url (no padding)
3. Standard base64

Minimum **32 decoded bytes**. Generate without embedding a live value:

```bash
python3 -c "import os,base64; print(base64.urlsafe_b64encode(os.urandom(32)).decode().rstrip('='))"
```

### Key rotation (`CCTP_ACCESS_TOKEN_HMAC_PREVIOUS_KEYS`)

Optional comma-separated list (max **2** entries) using the same encodings as the primary key.

- **New quotes** always derive idempotent tokens with the **current** key.
- **Idempotent replay** tries current then previous keys against the stored hash.
- Client-held tokens remain valid while their hash matches a row; rotation does not invalidate
  already-issued ephemeral tokens.
- If replay cannot be recovered (key rotated without previous ring), API returns
  `503 cctp_not_enabled` with a message to request a new quote — never a fresh invalid token.
- Operational procedure: add old key to `CCTP_ACCESS_TOKEN_HMAC_PREVIOUS_KEYS`, deploy, drain
  idempotency TTL, then remove the old key from the ring.

### Token loss / new quote

Losing the access token cannot be recovered from the API (hash-only storage). Request a new
non-idempotent quote (omit `Idempotency-Key`) or a new idempotency key. Prior transfers remain
in the saga store but are unreachable without the original token.

### Quote DB-write smoke

Successful idempotent finalize runs in one transaction: insert `cctp_transfers` row + mark
`cctp_quote_idempotency.state = completed`. No `response_json`, plaintext token, raw XDR,
message bytes, or attestation blobs are persisted.

## Verification

```bash
cargo test -p stellarroute-api --test api_v2_cctp_contract
cargo test -p stellarroute-api --test api_v2_cctp_http
TEST_DATABASE_URL=postgres://... cargo test -p stellarroute-api --test api_v2_cctp_http_pg -- --ignored
./scripts/cctp-pg-test.sh
npm --prefix sdk-js run test -- src/cctp.test.ts
```
