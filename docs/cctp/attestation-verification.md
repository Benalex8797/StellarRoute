# CCTP v2 Attestation Verification

## Trust model

- **On-chain destination `MessageTransmitterV2` attester set is authoritative** for `signatureThreshold` and enabled attester membership.
- **Iris `/v2/publicKeys`** is discovery/cross-check only; never used as threshold authority.
- **Cryptographic verification** mirrors Circle `Attestable` / Stellar `attestable::storage` rules:
  - digest = `keccak256(raw_message)` (no personal-sign prefix)
  - attestation length = `65 * signatureThreshold`
  - low-`s` enforced; `v` ∈ {27, 28, 0, 1}
  - recovered addresses strictly increasing; all must be enabled on destination

## Pinned sources

| Component | Source |
|-----------|--------|
| EVM rules | `circlefin/evm-cctp-contracts` `src/roles/Attestable.sol` @ master |
| Stellar rules | `circlefin/stellar-cctp` `packages/cctp-roles/src/attestable/storage.rs` @ master |
| Test vectors | `stellar-cctp` `packages/cctp-roles/src/test_utils/attestable.rs` @ master |
| Iris API | `GET /v2/publicKeys` (sandbox: `iris-api-sandbox.circle.com`) |

## Crypto dependencies

- `tiny-keccak` (Keccak-256) — dual MIT/Apache-2.0
- `k256` (secp256k1 recover) — Apache-2.0 / MIT

## Cache / rotation

| Cache | Default TTL | Max stale | Refresh |
|-------|-------------|-----------|---------|
| Iris public keys | 15m | 24h | scheduled + single-flight on unknown signer |
| Destination attester snapshots (Sepolia + Stellar) | 15m | 24h | atomic swap; fail closed beyond max stale |

Env overrides: `CCTP_IRIS_KEYS_TTL_SECS`, `CCTP_IRIS_KEYS_STALE_MAX_SECS`, `CCTP_ATTESTER_SNAPSHOT_TTL_SECS`, `CCTP_ATTESTER_SNAPSHOT_STALE_MAX_SECS`.

## Operator alerts

- `stellarroute_cctp_iris_keys_refresh_total{outcome="failure"}`
- `stellarroute_cctp_attester_snapshot_refresh_total{outcome="failure"}`
- `stellarroute_cctp_attestation_verify_total{reason!="ok"}`

## Public safety (current phase)

- HTTP handlers remain **503**; capabilities **false**.
- `CircleAttestationVerifier` may become **ready** when Iris + EVM + Stellar RPC readers bootstrap, but **corridor is not live** until Stellar burn/approval/mint verifiers ship.
- `is_public_executable()` stays **false** until all runtime components ready.
