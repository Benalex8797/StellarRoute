# Stellar CCTP verifier status (testnet)

Production Stellar approval, burn, and mint verifiers are implemented via Soroban RPC
`getTransaction` + pinned Circle `stellar-cctp` event layouts (`45746f2c8031`).

## Implemented

| Verifier | RPC evidence | On-chain binding |
|----------|--------------|------------------|
| `StellarRpcApprovalVerifier` | Finalized `SUCCESS` tx; single `approve` invoke on USDC | owner, spender=TokenMessenger, amount |
| `StellarRpcBurnVerifier` | `deposit_for_burn` invoke + `deposit_for_burn` event + `message_sent` | Cross-check invoke/event/message parser |
| `StellarRpcMintVerifier` | `mint_and_forward` invoke; completion via `mint_and_forward` / `message_received` / `is_nonce_used` | message+attestation bytes, payload hash |

## Still blocking `is_public_executable`

- Stellar **builders** (`ProductionStellarCctpBuilder`) remain separate readiness gate
- Attestation verifier bootstrap (Iris + attester snapshots) must be ready
- `CCTP_ENABLED` and full `CctpRuntime::assess` must pass for corridor direction

Public HTTP execution wiring is a later reviewed phase.

## Live fixture gap

No pinned successful Stellar Testnet CCTP burn/mint tx hash is checked into the repo yet.
Read-only probes (`#[ignore]` tests) verify RPC methods and `is_nonce_used` simulation only.
